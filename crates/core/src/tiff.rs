//! TIFF format probing, decoding and krilla integration (design §4.1.1, §4.3).

use std::io::{BufRead, Read, Seek};
use std::sync::Arc;

use image::metadata::Orientation;
use krilla::image::{BitsPerComponent, CustomImage, Image, ImageColorspace};
use tiff::decoder::{Decoder, Limits};
use tiff::tags::Tag;

use crate::error::{MAX_TIFF_PAGES, ProbeError};
use crate::probe::{DEFAULT_DPI, ImageFormat, ImageInfo, MAX_DPI, MAX_IMAGE_PIXELS, MIN_DPI};

/// Custom image implementation for krilla using raw decoded TIFF channels.
#[derive(Clone, Hash)]
pub struct CustomTiffImage {
    pub width: u32,
    pub height: u32,
    pub color_space: ImageColorspace,
    pub bits_per_component: BitsPerComponent,
    pub color_channel: Arc<Vec<u8>>,
    pub alpha_channel: Option<Arc<Vec<u8>>>,
    pub icc_profile: Option<Arc<Vec<u8>>>,
}

impl CustomImage for CustomTiffImage {
    fn color_channel(&self) -> &[u8] {
        &self.color_channel
    }

    fn alpha_channel(&self) -> Option<&[u8]> {
        self.alpha_channel.as_deref().map(|v| v.as_slice())
    }

    fn bits_per_component(&self) -> BitsPerComponent {
        self.bits_per_component
    }

    fn size(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    fn icc_profile(&self) -> Option<&[u8]> {
        self.icc_profile.as_deref().map(|v| v.as_slice())
    }

    fn color_space(&self) -> ImageColorspace {
        self.color_space
    }
}

/// Probes a TIFF reader, reading only IFD metadata across all pages (design §4.1.1).
///
/// Verifies:
/// - Total page count does not exceed [`MAX_TIFF_PAGES`].
/// - Total pixels per page does not exceed [`MAX_IMAGE_PIXELS`].
/// - Supported color and compression formats per design §4.1.1.
/// - Returns dimensions, orientation and resolution from the first page, and in
///   [`ImageInfo::decoded_bytes`] the pixel bytes of the largest page.
pub fn probe_tiff<R: BufRead + Seek>(reader: &mut R) -> Result<ImageInfo, ProbeError> {
    let mut decoder = Decoder::new(reader).map_err(|_| ProbeError::DecodeFailed)?;
    let mut limits = Limits::default();
    limits.decoding_buffer_size = (MAX_IMAGE_PIXELS * 8) as usize;
    decoder = decoder.with_limits(limits);

    let mut page_count: u32 = 0;
    let mut first_page: Option<(u32, u32, Orientation, u32, u32)> = None;
    let mut decoded_bytes: u64 = 0;

    loop {
        page_count += 1;
        if page_count > MAX_TIFF_PAGES {
            return Err(ProbeError::TooManyPages);
        }

        let (width, height) = decoder.dimensions().map_err(|_| ProbeError::DecodeFailed)?;
        let total_pixels = u64::from(width).saturating_mul(u64::from(height));
        if total_pixels > MAX_IMAGE_PIXELS {
            return Err(ProbeError::TooLarge);
        }

        validate_page_format(&mut decoder)?;

        let color_type = decoder.colortype().ok();
        let extra_sample_0 = read_extra_sample_0(&mut decoder)?;
        decoded_bytes = decoded_bytes.max(page_decoded_bytes(
            width,
            height,
            color_type,
            extra_sample_0,
        ));

        if first_page.is_none() {
            let orientation = read_tiff_orientation(&mut decoder)?;
            let (dpi_x, dpi_y) = read_tiff_dpi(&mut decoder)?;
            first_page = Some((width, height, orientation, dpi_x, dpi_y));
        }

        if decoder.more_images() {
            decoder.next_image().map_err(|_| ProbeError::DecodeFailed)?;
        } else {
            break;
        }
    }

    let (width, height, orientation, dpi_x, dpi_y) = first_page.ok_or(ProbeError::DecodeFailed)?;

    Ok(ImageInfo {
        format: ImageFormat::Tiff,
        width,
        height,
        orientation,
        dpi_x,
        dpi_y,
        page_count,
        decoded_bytes,
    })
}

/// The pixel bytes per pixel to assume when a page's colour type is not one that
/// [`process_decoded_samples`] reads: the largest it produces (16-bit RGBA).
const MAX_DECODED_BYTES_PER_PIXEL: u64 = 8;

/// Bytes per pixel of the channels [`process_decoded_samples`] returns for a page of
/// `color_type` (design §4.1.1): 1, 2 and 4-bit grey are expanded to 8 bits and YCbCr is
/// converted to RGB. Returns `None` for a colour type it does not read.
///
/// Keep in step with the arms of [`process_decoded_samples`]; the tests compare the two.
fn decoded_bytes_per_pixel(color_type: tiff::ColorType, extra_sample_0: u16) -> Option<u64> {
    let has_alpha = matches!(extra_sample_0, 1 | 2);
    match color_type {
        tiff::ColorType::Gray(1 | 2 | 4 | 8) => Some(1),
        tiff::ColorType::Gray(16) => Some(2),
        tiff::ColorType::Multiband { bit_depth: 8, .. } => Some(1 + u64::from(has_alpha)),
        tiff::ColorType::Multiband { bit_depth: 16, .. } => Some(2 * (1 + u64::from(has_alpha))),
        tiff::ColorType::RGB(8) | tiff::ColorType::YCbCr(8) => Some(3),
        tiff::ColorType::RGB(16) => Some(6),
        tiff::ColorType::RGBA(8) => Some(4),
        tiff::ColorType::RGBA(16) => Some(8),
        _ => None,
    }
}

/// Pixel bytes of one page once it is read (see [`ImageInfo::decoded_bytes`]).
fn page_decoded_bytes(
    width: u32,
    height: u32,
    color_type: Option<tiff::ColorType>,
    extra_sample_0: u16,
) -> u64 {
    let bytes_per_pixel = color_type
        .and_then(|color_type| decoded_bytes_per_pixel(color_type, extra_sample_0))
        .unwrap_or(MAX_DECODED_BYTES_PER_PIXEL);
    u64::from(width)
        .saturating_mul(u64::from(height))
        .saturating_mul(bytes_per_pixel)
}

/// The first value of the `ExtraSamples` tag of the current page, or 0 if it has none.
fn read_extra_sample_0<R: Read + Seek>(decoder: &mut Decoder<R>) -> Result<u16, ProbeError> {
    let extra_samples: Vec<u16> = decoder
        .find_tag_unsigned_vec(Tag::ExtraSamples)
        .map_err(|_| ProbeError::DecodeFailed)?
        .unwrap_or_default();
    Ok(extra_samples.first().copied().unwrap_or(0))
}

/// Validates that the current IFD directory represents a supported TIFF format (design §4.1.1).
fn validate_page_format<R: Read + Seek>(decoder: &mut Decoder<R>) -> Result<(), ProbeError> {
    let comp = decoder
        .find_tag_unsigned::<u16>(Tag::Compression)
        .map_err(|_| ProbeError::DecodeFailed)?
        .unwrap_or(1);
    match comp {
        1 /* None */
        | 4 /* Group4Fax */
        | 5 /* LZW */
        | 7 /* JPEG */
        | 8 /* Deflate */
        | 32946 /* TiffDeflate */
        | 32773 /* Packbits */ => {}
        _ => return Err(ProbeError::UnsupportedFormat),
    }

    let photo = decoder
        .find_tag_unsigned::<u16>(Tag::PhotometricInterpretation)
        .map_err(|_| ProbeError::DecodeFailed)?
        .ok_or(ProbeError::DecodeFailed)?;
    match photo {
        0..=2 /* WhiteIsZero, BlackIsZero, RGB */ => {}
        6 /* YCbCr */ => {
            if comp != 7 {
                return Err(ProbeError::UnsupportedFormat);
            }
        }
        _ => return Err(ProbeError::UnsupportedFormat),
    }

    let planar = decoder
        .find_tag_unsigned::<u16>(Tag::PlanarConfiguration)
        .map_err(|_| ProbeError::DecodeFailed)?
        .unwrap_or(1);
    if planar != 1 {
        return Err(ProbeError::UnsupportedFormat);
    }

    if decoder
        .find_tag_unsigned_vec::<u16>(Tag::SampleFormat)
        .map_err(|_| ProbeError::DecodeFailed)?
        .is_some_and(|formats| formats.iter().any(|&f| f != 1))
    {
        return Err(ProbeError::UnsupportedFormat);
    }

    let bits: Vec<u16> = decoder
        .find_tag_unsigned_vec(Tag::BitsPerSample)
        .map_err(|_| ProbeError::DecodeFailed)?
        .unwrap_or_else(|| vec![1]);

    let extra_samples: Vec<u16> = decoder
        .find_tag_unsigned_vec(Tag::ExtraSamples)
        .map_err(|_| ProbeError::DecodeFailed)?
        .unwrap_or_default();

    match photo {
        0 => {
            // WhiteIsZero with alpha is unsupported by tiff crate and excluded (design §4.1.1).
            if !extra_samples.is_empty() || bits.len() != 1 {
                return Err(ProbeError::UnsupportedFormat);
            }
            if !matches!(bits[0], 1 | 2 | 4 | 8 | 16) {
                return Err(ProbeError::UnsupportedFormat);
            }
        }
        1 => {
            if extra_samples.is_empty() {
                if bits.len() != 1 || !matches!(bits[0], 1 | 2 | 4 | 8 | 16) {
                    return Err(ProbeError::UnsupportedFormat);
                }
            } else {
                // BlackIsZero with alpha: 8 or 16 bits only.
                if bits.is_empty()
                    || !matches!(bits[0], 8 | 16)
                    || bits.iter().any(|&b| b != bits[0])
                {
                    return Err(ProbeError::UnsupportedFormat);
                }
            }
        }
        2 => {
            if bits.len() < 3 || !matches!(bits[0], 8 | 16) || bits.iter().any(|&b| b != bits[0]) {
                return Err(ProbeError::UnsupportedFormat);
            }
        }
        6 => {
            if bits.len() < 3 || bits.iter().take(3).any(|&b| b != 8) {
                return Err(ProbeError::UnsupportedFormat);
            }
        }
        _ => return Err(ProbeError::UnsupportedFormat),
    }

    Ok(())
}

fn read_tiff_orientation<R: Read + Seek>(
    decoder: &mut Decoder<R>,
) -> Result<Orientation, ProbeError> {
    let orient_val: Option<u16> = decoder
        .find_tag_unsigned(Tag::Orientation)
        .map_err(|_| ProbeError::DecodeFailed)?;
    Ok(match orient_val {
        Some(1) => Orientation::NoTransforms,
        Some(2) => Orientation::FlipHorizontal,
        Some(3) => Orientation::Rotate180,
        Some(4) => Orientation::FlipVertical,
        Some(5) => Orientation::Rotate90FlipH,
        Some(6) => Orientation::Rotate90,
        Some(7) => Orientation::Rotate270FlipH,
        Some(8) => Orientation::Rotate270,
        _ => Orientation::NoTransforms,
    })
}

fn parse_rational(val: &tiff::decoder::ifd::Value) -> Option<(u32, u32)> {
    match val {
        tiff::decoder::ifd::Value::Rational(n, d) => Some((*n, *d)),
        tiff::decoder::ifd::Value::List(list) => {
            if let Some(tiff::decoder::ifd::Value::Rational(n, d)) = list.first() {
                Some((*n, *d))
            } else {
                None
            }
        }
        _ => None,
    }
}

fn read_tiff_dpi<R: Read + Seek>(decoder: &mut Decoder<R>) -> Result<(u32, u32), ProbeError> {
    let x_res = decoder
        .find_tag(Tag::XResolution)
        .map_err(|_| ProbeError::DecodeFailed)?
        .and_then(|v| parse_rational(&v));
    let y_res = decoder
        .find_tag(Tag::YResolution)
        .map_err(|_| ProbeError::DecodeFailed)?
        .and_then(|v| parse_rational(&v));
    let unit: u16 = decoder
        .find_tag_unsigned(Tag::ResolutionUnit)
        .map_err(|_| ProbeError::DecodeFailed)?
        .unwrap_or(2);

    let raw_dpi = match (x_res, y_res, unit) {
        (Some((x_num, x_den)), Some((y_num, y_den)), 2) if x_den > 0 && y_den > 0 => {
            let dx = (f64::from(x_num) / f64::from(x_den)).round() as u32;
            let dy = (f64::from(y_num) / f64::from(y_den)).round() as u32;
            Some((dx, dy))
        }
        (Some((x_num, x_den)), Some((y_num, y_den)), 3) if x_den > 0 && y_den > 0 => {
            let dx = ((f64::from(x_num) / f64::from(x_den)) * 2.54).round() as u32;
            let dy = ((f64::from(y_num) / f64::from(y_den)) * 2.54).round() as u32;
            Some((dx, dy))
        }
        _ => None,
    };

    let dpi = match raw_dpi {
        Some((dx, dy))
            if (MIN_DPI..=MAX_DPI).contains(&dx) && (MIN_DPI..=MAX_DPI).contains(&dy) =>
        {
            (dx, dy)
        }
        _ => (DEFAULT_DPI, DEFAULT_DPI),
    };
    Ok(dpi)
}

/// Decodes all pages of a TIFF file into krilla `Image`s and layout `ImageInfo`s atomically.
///
/// Progress callback `on_page` is invoked as each page is decoded: `on_page(page_index, total_pages)`.
/// `should_cancel` is checked between pages. If cancelled, returns `Ok(Err(()))` without committing.
/// If decoding any page fails, returns `Err(ProbeError::DecodeFailed)` without committing.
pub fn decode_tiff_pages<R: BufRead + Seek, FProg, FCancel>(
    reader: &mut R,
    interpolate: bool,
    mut on_page: FProg,
    should_cancel: FCancel,
) -> Result<Result<Vec<(Image, ImageInfo)>, ()>, ProbeError>
where
    FProg: FnMut(u32, u32),
    FCancel: Fn() -> bool,
{
    // Probe first to validate headers across all pages and count total pages.
    reader.rewind().map_err(|_| ProbeError::ReadFailed)?;
    let probed = probe_tiff(reader)?;
    let total_pages = probed.page_count;

    reader.rewind().map_err(|_| ProbeError::ReadFailed)?;
    let mut decoder = Decoder::new(reader).map_err(|_| ProbeError::DecodeFailed)?;
    let mut limits = Limits::default();
    limits.decoding_buffer_size = (MAX_IMAGE_PIXELS * 8) as usize;
    decoder = decoder.with_limits(limits);

    let mut pages = Vec::with_capacity(total_pages as usize);

    for page_idx in 0..total_pages {
        if should_cancel() {
            return Ok(Err(()));
        }

        let (image, info) = decode_single_page(&mut decoder, total_pages, interpolate)?;
        pages.push((image, info));

        on_page(page_idx, total_pages);

        if page_idx + 1 < total_pages {
            if should_cancel() {
                return Ok(Err(()));
            }
            decoder.next_image().map_err(|_| ProbeError::DecodeFailed)?;
        }
    }

    Ok(Ok(pages))
}

/// Decodes the current page IFD into a krilla `Image` and an `ImageInfo`.
fn decode_single_page<R: Read + Seek>(
    decoder: &mut Decoder<R>,
    total_pages: u32,
    interpolate: bool,
) -> Result<(Image, ImageInfo), ProbeError> {
    let (width, height) = decoder.dimensions().map_err(|_| ProbeError::DecodeFailed)?;
    let orientation = read_tiff_orientation(decoder)?;
    let (dpi_x, dpi_y) = read_tiff_dpi(decoder)?;

    let extra_sample_0 = read_extra_sample_0(decoder)?;

    let icc = decoder
        .find_tag(Tag::IccProfile)
        .ok()
        .flatten()
        .and_then(|v| v.into_u8_vec().ok());

    let color_type = decoder.colortype().map_err(|_| ProbeError::DecodeFailed)?;
    let raw = decoder.read_image().map_err(|_| ProbeError::DecodeFailed)?;

    let (color_space, bits_per_component, color_channel, alpha_channel) =
        process_decoded_samples(width, height, color_type, extra_sample_0, raw)?;

    let filtered_icc = match (color_space, icc) {
        (ImageColorspace::Luma, Some(data)) if data.len() >= 20 && &data[16..20] == b"GRAY" => {
            Some(data)
        }
        (ImageColorspace::Rgb, Some(data)) if data.len() >= 20 && &data[16..20] == b"RGB " => {
            Some(data)
        }
        _ => None,
    };

    let custom = CustomTiffImage {
        width,
        height,
        color_space,
        bits_per_component,
        color_channel: Arc::new(color_channel),
        alpha_channel: alpha_channel.map(Arc::new),
        icc_profile: filtered_icc.map(Arc::new),
    };

    let krilla_image =
        Image::from_custom(custom, interpolate).map_err(|_| ProbeError::DecodeFailed)?;

    let info = ImageInfo {
        format: ImageFormat::Tiff,
        width,
        height,
        orientation,
        dpi_x,
        dpi_y,
        page_count: total_pages,
        decoded_bytes: page_decoded_bytes(width, height, Some(color_type), extra_sample_0),
    };

    Ok((krilla_image, info))
}

type DecodedChannels = (ImageColorspace, BitsPerComponent, Vec<u8>, Option<Vec<u8>>);

fn process_decoded_samples(
    width: u32,
    height: u32,
    color_type: tiff::ColorType,
    extra_sample_0: u16,
    raw: tiff::decoder::DecodingResult,
) -> Result<DecodedChannels, ProbeError> {
    let total_pixels = usize::try_from(u64::from(width).saturating_mul(u64::from(height)))
        .map_err(|_| ProbeError::TooLarge)?;

    match color_type {
        tiff::ColorType::Gray(bit_depth) => {
            let u8_data = match raw {
                tiff::decoder::DecodingResult::U8(v) => v,
                tiff::decoder::DecodingResult::U16(v) => {
                    if bit_depth == 16 {
                        if v.len() != total_pixels {
                            return Err(ProbeError::DecodeFailed);
                        }
                        let mut color_channel = Vec::with_capacity(total_pixels * 2);
                        for sample in v {
                            color_channel.extend_from_slice(&sample.to_be_bytes());
                        }
                        return Ok((
                            ImageColorspace::Luma,
                            BitsPerComponent::Sixteen,
                            color_channel,
                            None,
                        ));
                    }
                    return Err(ProbeError::DecodeFailed);
                }
                _ => return Err(ProbeError::DecodeFailed),
            };

            match bit_depth {
                1 => {
                    let row_bytes = (width as usize).div_ceil(8);
                    if u8_data.len() < row_bytes * height as usize {
                        return Err(ProbeError::DecodeFailed);
                    }
                    let mut color_channel = Vec::with_capacity(total_pixels);
                    for y in 0..height as usize {
                        let row_start = y * row_bytes;
                        for x in 0..width as usize {
                            let byte = u8_data[row_start + x / 8];
                            let bit = (byte >> (7 - (x % 8))) & 1;
                            color_channel.push(if bit == 1 { 255 } else { 0 });
                        }
                    }
                    Ok((
                        ImageColorspace::Luma,
                        BitsPerComponent::Eight,
                        color_channel,
                        None,
                    ))
                }
                2 => {
                    let row_bytes = (width as usize * 2).div_ceil(8);
                    if u8_data.len() < row_bytes * height as usize {
                        return Err(ProbeError::DecodeFailed);
                    }
                    let mut color_channel = Vec::with_capacity(total_pixels);
                    for y in 0..height as usize {
                        let row_start = y * row_bytes;
                        for x in 0..width as usize {
                            let byte = u8_data[row_start + x / 4];
                            let shift = (3 - (x % 4)) * 2;
                            let val = ((byte >> shift) & 0x03) * 85;
                            color_channel.push(val);
                        }
                    }
                    Ok((
                        ImageColorspace::Luma,
                        BitsPerComponent::Eight,
                        color_channel,
                        None,
                    ))
                }
                4 => {
                    let row_bytes = (width as usize * 4).div_ceil(8);
                    if u8_data.len() < row_bytes * height as usize {
                        return Err(ProbeError::DecodeFailed);
                    }
                    let mut color_channel = Vec::with_capacity(total_pixels);
                    for y in 0..height as usize {
                        let row_start = y * row_bytes;
                        for x in 0..width as usize {
                            let byte = u8_data[row_start + x / 2];
                            let shift = (1 - (x % 2)) * 4;
                            let val = ((byte >> shift) & 0x0F) * 17;
                            color_channel.push(val);
                        }
                    }
                    Ok((
                        ImageColorspace::Luma,
                        BitsPerComponent::Eight,
                        color_channel,
                        None,
                    ))
                }
                8 => {
                    if u8_data.len() != total_pixels {
                        return Err(ProbeError::DecodeFailed);
                    }
                    Ok((
                        ImageColorspace::Luma,
                        BitsPerComponent::Eight,
                        u8_data,
                        None,
                    ))
                }
                _ => Err(ProbeError::UnsupportedFormat),
            }
        }
        tiff::ColorType::Multiband {
            bit_depth: 8,
            num_samples,
        } => {
            let u8_data = match raw {
                tiff::decoder::DecodingResult::U8(v) => v,
                _ => return Err(ProbeError::DecodeFailed),
            };
            let stride = num_samples as usize;
            if stride < 2 || u8_data.len() != total_pixels * stride {
                return Err(ProbeError::DecodeFailed);
            }
            let mut color_channel = Vec::with_capacity(total_pixels);
            let has_alpha = extra_sample_0 == 1 || extra_sample_0 == 2;
            let mut alpha_channel = if has_alpha {
                Some(Vec::with_capacity(total_pixels))
            } else {
                None
            };

            for i in 0..total_pixels {
                let g = u8_data[i * stride];
                if let Some(alpha) = alpha_channel.as_mut() {
                    let a = u8_data[i * stride + 1];
                    let final_g = if extra_sample_0 == 1 {
                        if a == 0 {
                            0
                        } else {
                            ((u32::from(g) * 255 + u32::from(a) / 2) / u32::from(a)).min(255) as u8
                        }
                    } else {
                        g
                    };
                    color_channel.push(final_g);
                    alpha.push(a);
                } else {
                    color_channel.push(g);
                }
            }

            Ok((
                ImageColorspace::Luma,
                BitsPerComponent::Eight,
                color_channel,
                alpha_channel,
            ))
        }
        tiff::ColorType::Multiband {
            bit_depth: 16,
            num_samples,
        } => {
            let u16_data = match raw {
                tiff::decoder::DecodingResult::U16(v) => v,
                _ => return Err(ProbeError::DecodeFailed),
            };
            let stride = num_samples as usize;
            if stride < 2 || u16_data.len() != total_pixels * stride {
                return Err(ProbeError::DecodeFailed);
            }
            let mut color_channel = Vec::with_capacity(total_pixels * 2);
            let has_alpha = extra_sample_0 == 1 || extra_sample_0 == 2;
            let mut alpha_channel = if has_alpha {
                Some(Vec::with_capacity(total_pixels * 2))
            } else {
                None
            };

            for i in 0..total_pixels {
                let g = u16_data[i * stride];
                if let Some(alpha) = alpha_channel.as_mut() {
                    let a = u16_data[i * stride + 1];
                    let final_g = if extra_sample_0 == 1 {
                        if a == 0 {
                            0
                        } else {
                            ((u64::from(g) * 65535 + u64::from(a) / 2) / u64::from(a)).min(65535)
                                as u16
                        }
                    } else {
                        g
                    };
                    color_channel.extend_from_slice(&final_g.to_be_bytes());
                    alpha.extend_from_slice(&a.to_be_bytes());
                } else {
                    color_channel.extend_from_slice(&g.to_be_bytes());
                }
            }

            Ok((
                ImageColorspace::Luma,
                BitsPerComponent::Sixteen,
                color_channel,
                alpha_channel,
            ))
        }
        tiff::ColorType::RGB(8) => {
            let u8_data = match raw {
                tiff::decoder::DecodingResult::U8(v) => v,
                _ => return Err(ProbeError::DecodeFailed),
            };
            if u8_data.len() != total_pixels * 3 {
                return Err(ProbeError::DecodeFailed);
            }
            Ok((ImageColorspace::Rgb, BitsPerComponent::Eight, u8_data, None))
        }
        tiff::ColorType::RGB(16) => {
            let u16_data = match raw {
                tiff::decoder::DecodingResult::U16(v) => v,
                _ => return Err(ProbeError::DecodeFailed),
            };
            if u16_data.len() != total_pixels * 3 {
                return Err(ProbeError::DecodeFailed);
            }
            let mut color_channel = Vec::with_capacity(total_pixels * 6);
            for sample in u16_data {
                color_channel.extend_from_slice(&sample.to_be_bytes());
            }
            Ok((
                ImageColorspace::Rgb,
                BitsPerComponent::Sixteen,
                color_channel,
                None,
            ))
        }
        tiff::ColorType::RGBA(8) => {
            let u8_data = match raw {
                tiff::decoder::DecodingResult::U8(v) => v,
                _ => return Err(ProbeError::DecodeFailed),
            };
            if u8_data.len() != total_pixels * 4 {
                return Err(ProbeError::DecodeFailed);
            }
            let mut color_channel = Vec::with_capacity(total_pixels * 3);
            let mut alpha_channel = Vec::with_capacity(total_pixels);

            for chunk in u8_data.as_chunks::<4>().0 {
                let [r, g, b, a] = *chunk;
                let unmult = |c: u8| -> u8 {
                    if a == 0 {
                        0
                    } else {
                        ((u32::from(c) * 255 + u32::from(a) / 2) / u32::from(a)).min(255) as u8
                    }
                };
                let (fr, fg, fb) = if extra_sample_0 == 1 {
                    (unmult(r), unmult(g), unmult(b))
                } else {
                    (r, g, b)
                };
                color_channel.push(fr);
                color_channel.push(fg);
                color_channel.push(fb);
                alpha_channel.push(a);
            }

            Ok((
                ImageColorspace::Rgb,
                BitsPerComponent::Eight,
                color_channel,
                Some(alpha_channel),
            ))
        }
        tiff::ColorType::RGBA(16) => {
            let u16_data = match raw {
                tiff::decoder::DecodingResult::U16(v) => v,
                _ => return Err(ProbeError::DecodeFailed),
            };
            if u16_data.len() != total_pixels * 4 {
                return Err(ProbeError::DecodeFailed);
            }
            let mut color_channel = Vec::with_capacity(total_pixels * 6);
            let mut alpha_channel = Vec::with_capacity(total_pixels * 2);

            for chunk in u16_data.as_chunks::<4>().0 {
                let [r, g, b, a] = *chunk;
                let unmult = |c: u16| -> u16 {
                    if a == 0 {
                        0
                    } else {
                        ((u64::from(c) * 65535 + u64::from(a) / 2) / u64::from(a)).min(65535) as u16
                    }
                };
                let (fr, fg, fb) = if extra_sample_0 == 1 {
                    (unmult(r), unmult(g), unmult(b))
                } else {
                    (r, g, b)
                };
                color_channel.extend_from_slice(&fr.to_be_bytes());
                color_channel.extend_from_slice(&fg.to_be_bytes());
                color_channel.extend_from_slice(&fb.to_be_bytes());
                alpha_channel.extend_from_slice(&a.to_be_bytes());
            }

            Ok((
                ImageColorspace::Rgb,
                BitsPerComponent::Sixteen,
                color_channel,
                Some(alpha_channel),
            ))
        }
        tiff::ColorType::YCbCr(8) => {
            let u8_data = match raw {
                tiff::decoder::DecodingResult::U8(v) => v,
                _ => return Err(ProbeError::DecodeFailed),
            };
            if u8_data.len() != total_pixels * 3 {
                return Err(ProbeError::DecodeFailed);
            }
            let mut color_channel = Vec::with_capacity(total_pixels * 3);

            for chunk in u8_data.as_chunks::<3>().0 {
                let [y_val, cb_val, cr_val] = *chunk;
                let y = f64::from(y_val);
                let cb = f64::from(cb_val) - 128.0;
                let cr = f64::from(cr_val) - 128.0;

                let r = (y + 1.402 * cr).round().clamp(0.0, 255.0) as u8;
                let g = (y - 0.344136 * cb - 0.714136 * cr)
                    .round()
                    .clamp(0.0, 255.0) as u8;
                let b = (y + 1.772 * cb).round().clamp(0.0, 255.0) as u8;

                color_channel.push(r);
                color_channel.push(g);
                color_channel.push(b);
            }

            Ok((
                ImageColorspace::Rgb,
                BitsPerComponent::Eight,
                color_channel,
                None,
            ))
        }
        _ => Err(ProbeError::UnsupportedFormat),
    }
}

/// Decodes the first page of a TIFF file into an [`image::DynamicImage`] for thumbnail generation.
pub fn decode_tiff_first_page_as_image(
    image_bytes: &[u8],
) -> Result<image::DynamicImage, ProbeError> {
    let mut reader = std::io::Cursor::new(image_bytes);
    let mut decoder = Decoder::new(&mut reader).map_err(|_| ProbeError::DecodeFailed)?;
    let mut limits = Limits::default();
    limits.decoding_buffer_size = (MAX_IMAGE_PIXELS * 8) as usize;
    decoder = decoder.with_limits(limits);

    let (width, height) = decoder.dimensions().map_err(|_| ProbeError::DecodeFailed)?;
    let total_pixels = u64::from(width).saturating_mul(u64::from(height));
    if total_pixels > MAX_IMAGE_PIXELS {
        return Err(ProbeError::TooLarge);
    }
    validate_page_format(&mut decoder)?;

    let extra_sample_0 = read_extra_sample_0(&mut decoder)?;

    let color_type = decoder.colortype().map_err(|_| ProbeError::DecodeFailed)?;
    let raw = decoder.read_image().map_err(|_| ProbeError::DecodeFailed)?;

    let (color_space, bits_per_component, color_channel, alpha_channel) =
        process_decoded_samples(width, height, color_type, extra_sample_0, raw)?;

    match (color_space, bits_per_component, alpha_channel) {
        (ImageColorspace::Luma, BitsPerComponent::Eight, None) => {
            let img = image::GrayImage::from_raw(width, height, color_channel)
                .ok_or(ProbeError::DecodeFailed)?;
            Ok(image::DynamicImage::ImageLuma8(img))
        }
        (ImageColorspace::Luma, BitsPerComponent::Eight, Some(alpha)) => {
            let mut combined = Vec::with_capacity((width * height * 2) as usize);
            for (g, a) in color_channel.into_iter().zip(alpha) {
                combined.push(g);
                combined.push(a);
            }
            let img = image::GrayAlphaImage::from_raw(width, height, combined)
                .ok_or(ProbeError::DecodeFailed)?;
            Ok(image::DynamicImage::ImageLumaA8(img))
        }
        (ImageColorspace::Luma, BitsPerComponent::Sixteen, None) => {
            // Convert BE bytes back to u16 in native endianness for image crate
            let samples: Vec<u16> = color_channel
                .as_chunks::<2>()
                .0
                .iter()
                .map(|&[b0, b1]| u16::from_be_bytes([b0, b1]))
                .collect();
            let img = image::ImageBuffer::from_raw(width, height, samples)
                .ok_or(ProbeError::DecodeFailed)?;
            Ok(image::DynamicImage::ImageLuma16(img))
        }
        (ImageColorspace::Luma, BitsPerComponent::Sixteen, Some(alpha)) => {
            let g_samples = color_channel
                .as_chunks::<2>()
                .0
                .iter()
                .map(|&[b0, b1]| u16::from_be_bytes([b0, b1]));
            let a_samples = alpha
                .as_chunks::<2>()
                .0
                .iter()
                .map(|&[b0, b1]| u16::from_be_bytes([b0, b1]));
            let mut combined = Vec::with_capacity((width * height * 2) as usize);
            for (g, a) in g_samples.zip(a_samples) {
                combined.push(g);
                combined.push(a);
            }
            let img = image::ImageBuffer::from_raw(width, height, combined)
                .ok_or(ProbeError::DecodeFailed)?;
            Ok(image::DynamicImage::ImageLumaA16(img))
        }
        (ImageColorspace::Rgb, BitsPerComponent::Eight, None) => {
            let img = image::RgbImage::from_raw(width, height, color_channel)
                .ok_or(ProbeError::DecodeFailed)?;
            Ok(image::DynamicImage::ImageRgb8(img))
        }
        (ImageColorspace::Rgb, BitsPerComponent::Eight, Some(alpha)) => {
            let mut combined = Vec::with_capacity((width * height * 4) as usize);
            let (rgb_chunks, _) = color_channel.as_chunks::<3>();
            for (&[r, g, b], a) in rgb_chunks.iter().zip(alpha) {
                combined.push(r);
                combined.push(g);
                combined.push(b);
                combined.push(a);
            }
            let img = image::RgbaImage::from_raw(width, height, combined)
                .ok_or(ProbeError::DecodeFailed)?;
            Ok(image::DynamicImage::ImageRgba8(img))
        }
        (ImageColorspace::Rgb, BitsPerComponent::Sixteen, None) => {
            let samples: Vec<u16> = color_channel
                .as_chunks::<2>()
                .0
                .iter()
                .map(|&[b0, b1]| u16::from_be_bytes([b0, b1]))
                .collect();
            let img = image::ImageBuffer::from_raw(width, height, samples)
                .ok_or(ProbeError::DecodeFailed)?;
            Ok(image::DynamicImage::ImageRgb16(img))
        }
        (ImageColorspace::Rgb, BitsPerComponent::Sixteen, Some(alpha)) => {
            let rgb_samples: Vec<u16> = color_channel
                .as_chunks::<2>()
                .0
                .iter()
                .map(|&[b0, b1]| u16::from_be_bytes([b0, b1]))
                .collect();
            let a_samples: Vec<u16> = alpha
                .as_chunks::<2>()
                .0
                .iter()
                .map(|&[b0, b1]| u16::from_be_bytes([b0, b1]))
                .collect();
            let mut combined = Vec::with_capacity((width * height * 4) as usize);
            let (rgb_triplets, _) = rgb_samples.as_chunks::<3>();
            for (&[r, g, b], a) in rgb_triplets.iter().zip(a_samples) {
                combined.push(r);
                combined.push(g);
                combined.push(b);
                combined.push(a);
            }
            let img = image::ImageBuffer::from_raw(width, height, combined)
                .ok_or(ProbeError::DecodeFailed)?;
            Ok(image::DynamicImage::ImageRgba16(img))
        }
        _ => Err(ProbeError::UnsupportedFormat),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;
    use std::path::Path;

    const FIXTURES_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures");

    #[test]
    fn probe_detects_too_many_pages() {
        let path = Path::new(FIXTURES_DIR).join("too_many_pages.tif");
        let mut file = std::io::BufReader::new(std::fs::File::open(path).unwrap());
        assert_eq!(probe_tiff(&mut file), Err(ProbeError::TooManyPages));
    }

    #[test]
    fn probe_detects_too_large_on_page2() {
        let path = Path::new(FIXTURES_DIR).join("too_large_page2.tif");
        let mut file = std::io::BufReader::new(std::fs::File::open(path).unwrap());
        assert_eq!(probe_tiff(&mut file), Err(ProbeError::TooLarge));
    }

    #[test]
    fn probe_refuses_unsupported_fixture_formats() {
        for name in ["cmyk.tif", "fax_g3.tif", "planar.tif", "palette.tif"] {
            let path = Path::new(FIXTURES_DIR).join(name);
            let mut file = std::io::BufReader::new(std::fs::File::open(path).unwrap());
            assert_eq!(
                probe_tiff(&mut file),
                Err(ProbeError::UnsupportedFormat),
                "{name} must be UnsupportedFormat"
            );
        }
    }

    #[test]
    fn probe_corrupt_page2_passes_probe() {
        let path = Path::new(FIXTURES_DIR).join("corrupt_page2.tif");
        let mut file = std::io::BufReader::new(std::fs::File::open(path).unwrap());
        let info = probe_tiff(&mut file).expect("corrupt_page2 has valid IFDs and passes probe");
        assert_eq!(info.page_count, 2);
    }

    #[test]
    fn probe_cycle_in_ifd_chain_fails_with_decode_failed() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"II");
        bytes.extend_from_slice(&42u16.to_le_bytes());
        bytes.extend_from_slice(&8u32.to_le_bytes());

        bytes.extend_from_slice(&1u16.to_le_bytes());
        bytes.extend_from_slice(&256u16.to_le_bytes());
        bytes.extend_from_slice(&3u16.to_le_bytes());
        bytes.extend_from_slice(&1u32.to_le_bytes());
        bytes.extend_from_slice(&10u16.to_le_bytes());
        bytes.extend_from_slice(&[0, 0]);
        // Next IFD offset points back to 8 (cycle)
        bytes.extend_from_slice(&8u32.to_le_bytes());

        let mut cursor = Cursor::new(bytes);
        assert_eq!(probe_tiff(&mut cursor), Err(ProbeError::DecodeFailed));
    }

    #[test]
    fn probe_unsupported_white_is_zero_with_alpha() {
        let bytes = make_minimal_tiff(0, &[8, 8], Some(2), None);
        let mut cursor = Cursor::new(bytes);
        assert_eq!(probe_tiff(&mut cursor), Err(ProbeError::UnsupportedFormat));
    }

    #[test]
    fn probe_unsupported_float_sample_format() {
        let bytes = make_minimal_tiff(1, &[32], None, Some(3));
        let mut cursor = Cursor::new(bytes);
        assert_eq!(probe_tiff(&mut cursor), Err(ProbeError::UnsupportedFormat));
    }

    /// Fixtures that pass `probe_tiff` and that `decode_tiff_pages` reads.
    const READABLE_TIFF_FIXTURES: &[&str] = &[
        "scan_g4.tif",
        "fax_standard.tif",
        "gray_lzw.tif",
        "gray16_deflate.tif",
        "gray4_packbits.tif",
        "rgb_lzw.tif",
        "rgb16.tif",
        "rgba_unassoc.tif",
        "rgba_assoc.tif",
        "gray_alpha.tif",
        "photo_jpeg.tif",
        "rotate90.tif",
        "metadata.tif",
    ];

    /// Bytes of the colour and alpha channels `process_decoded_samples` returns for each
    /// page, read the way `decode_single_page` reads them.
    fn produced_page_bytes(bytes: &[u8]) -> Vec<u64> {
        let mut decoder = Decoder::new(Cursor::new(bytes)).expect("open tiff");
        let mut sizes = Vec::new();
        loop {
            let (width, height) = decoder.dimensions().expect("dimensions");
            let extra_sample_0 = read_extra_sample_0(&mut decoder).expect("extra samples");
            let color_type = decoder.colortype().expect("colortype");
            let raw = decoder.read_image().expect("read page");
            let (_, _, color, alpha) =
                process_decoded_samples(width, height, color_type, extra_sample_0, raw)
                    .expect("process page");
            sizes.push((color.len() + alpha.map_or(0, |a| a.len())) as u64);
            if decoder.more_images() {
                decoder.next_image().expect("next page");
            } else {
                return sizes;
            }
        }
    }

    #[test]
    fn decoded_bytes_is_the_largest_page_the_reader_produces_for_every_fixture() {
        for name in READABLE_TIFF_FIXTURES {
            let bytes = std::fs::read(Path::new(FIXTURES_DIR).join(name)).unwrap();
            let produced = produced_page_bytes(&bytes);
            let info = probe_tiff(&mut Cursor::new(&bytes)).unwrap();
            let largest = produced.iter().copied().max().unwrap();
            assert!(largest > 0, "{name}");
            assert_eq!(info.decoded_bytes, largest, "probe_tiff for {name}");

            let pages = decode_tiff_pages(&mut Cursor::new(&bytes), true, |_, _| {}, || false)
                .unwrap()
                .unwrap();
            let per_page: Vec<u64> = pages.iter().map(|(_, i)| i.decoded_bytes).collect();
            assert_eq!(per_page, produced, "decode_tiff_pages for {name}");
        }
    }

    #[test]
    fn decoded_bytes_of_a_ycbcr_fixture_is_rgb() {
        let bytes = std::fs::read(Path::new(FIXTURES_DIR).join("photo_jpeg.tif")).unwrap();
        let info = probe_tiff(&mut Cursor::new(&bytes)).unwrap();
        assert_eq!(info.decoded_bytes, 160 * 120 * 3);
    }

    struct TestPage {
        width: u32,
        height: u32,
        photo: u16,
        bits: Vec<u16>,
        extra: Vec<u16>,
    }

    impl TestPage {
        fn new(width: u32, height: u32, photo: u16, bits: &[u16], extra: &[u16]) -> Self {
            Self {
                width,
                height,
                photo,
                bits: bits.to_vec(),
                extra: extra.to_vec(),
            }
        }
    }

    /// An uncompressed little-endian TIFF with one zero-filled page per entry of `pages`.
    fn make_tiff_pages(pages: &[TestPage]) -> Vec<u8> {
        let mut bytes = b"II".to_vec();
        bytes.extend_from_slice(&42u16.to_le_bytes());
        bytes.extend_from_slice(&8u32.to_le_bytes());

        for (index, page) in pages.iter().enumerate() {
            let bits_per_pixel: u32 = page.bits.iter().map(|&b| u32::from(b)).sum();
            let row_bytes = (page.width * bits_per_pixel).div_ceil(8);
            let pixels = vec![0u8; (row_bytes * page.height) as usize];

            let shorts = |values: &[u16]| -> Vec<u8> {
                values.iter().flat_map(|v| v.to_le_bytes()).collect()
            };
            let mut entries: Vec<(u16, u16, u32, Vec<u8>)> = vec![
                (256, 4, 1, page.width.to_le_bytes().to_vec()),
                (257, 4, 1, page.height.to_le_bytes().to_vec()),
                (258, 3, page.bits.len() as u32, shorts(&page.bits)),
                (259, 3, 1, shorts(&[1])),
                (262, 3, 1, shorts(&[page.photo])),
                (273, 4, 1, Vec::new()),
                (277, 3, 1, shorts(&[page.bits.len() as u16])),
                (278, 4, 1, page.height.to_le_bytes().to_vec()),
                (279, 4, 1, (pixels.len() as u32).to_le_bytes().to_vec()),
            ];
            if !page.extra.is_empty() {
                entries.push((338, 3, page.extra.len() as u32, shorts(&page.extra)));
            }
            entries.sort_by_key(|e| e.0);

            let ifd_start = bytes.len();
            let overflow_start = ifd_start + 2 + entries.len() * 12 + 4;
            let mut overflow = Vec::new();
            let mut fields = Vec::new();
            for (tag, field_type, count, value) in &entries {
                let mut field = [0u8; 4];
                if value.len() <= 4 {
                    field[..value.len()].copy_from_slice(value);
                } else {
                    field = ((overflow_start + overflow.len()) as u32).to_le_bytes();
                    overflow.extend_from_slice(value);
                }
                fields.push((*tag, *field_type, *count, field));
            }
            let strip_start = overflow_start + overflow.len();
            let next_ifd = if index + 1 == pages.len() {
                0
            } else {
                (strip_start + pixels.len()) as u32
            };

            bytes.extend_from_slice(&(entries.len() as u16).to_le_bytes());
            for (tag, field_type, count, field) in fields {
                bytes.extend_from_slice(&tag.to_le_bytes());
                bytes.extend_from_slice(&field_type.to_le_bytes());
                bytes.extend_from_slice(&count.to_le_bytes());
                if tag == 273 {
                    bytes.extend_from_slice(&(strip_start as u32).to_le_bytes());
                } else {
                    bytes.extend_from_slice(&field);
                }
            }
            bytes.extend_from_slice(&next_ifd.to_le_bytes());
            bytes.extend_from_slice(&overflow);
            bytes.extend_from_slice(&pixels);
        }
        bytes
    }

    #[test]
    fn decoded_bytes_per_pixel_for_each_colour_type() {
        // (name, photometric, bits per sample, extra samples, bytes per pixel)
        type Case<'a> = (&'a str, u16, &'a [u16], &'a [u16], u64);
        let cases: &[Case] = &[
            ("black and white (1-bit grey)", 1, &[1], &[], 1),
            ("2-bit grey", 1, &[2], &[], 1),
            ("4-bit grey", 1, &[4], &[], 1),
            ("8-bit grey", 1, &[8], &[], 1),
            ("16-bit grey", 1, &[16], &[], 2),
            ("white is zero 1-bit", 0, &[1], &[], 1),
            ("white is zero 8-bit", 0, &[8], &[], 1),
            ("grey and associated alpha 8", 1, &[8, 8], &[1], 2),
            ("grey and unassociated alpha 8", 1, &[8, 8], &[2], 2),
            ("grey and alpha 16", 1, &[16, 16], &[2], 4),
            ("grey with an unspecified extra sample", 1, &[8, 8], &[0], 1),
            ("RGB 8", 2, &[8, 8, 8], &[], 3),
            ("RGB 16", 2, &[16, 16, 16], &[], 6),
            ("RGBA 8", 2, &[8, 8, 8, 8], &[2], 4),
            ("RGBA 8 associated", 2, &[8, 8, 8, 8], &[1], 4),
            ("RGBA 16", 2, &[16, 16, 16, 16], &[2], 8),
            ("RGBA 16 associated", 2, &[16, 16, 16, 16], &[1], 8),
            (
                "RGB 8 with an unspecified extra sample",
                2,
                &[8, 8, 8, 8],
                &[0],
                3,
            ),
        ];
        for &(name, photo, bits, extra, bytes_per_pixel) in cases {
            let tiff = make_tiff_pages(&[TestPage::new(7, 5, photo, bits, extra)]);
            let info =
                probe_tiff(&mut Cursor::new(&tiff)).unwrap_or_else(|e| panic!("{name}: {e:?}"));
            assert_eq!(
                info.decoded_bytes,
                7 * 5 * bytes_per_pixel,
                "probe for {name}"
            );
            assert_eq!(
                produced_page_bytes(&tiff),
                [7 * 5 * bytes_per_pixel],
                "reader for {name}"
            );
        }
    }

    #[test]
    fn decoded_bytes_is_the_largest_page_by_bytes_not_by_pixels() {
        // The first page has the most pixels but the fewest bytes; the last has the most bytes.
        let pages = [
            TestPage::new(20, 20, 1, &[8], &[]),
            TestPage::new(10, 10, 2, &[16, 16, 16, 16], &[2]),
            TestPage::new(15, 15, 2, &[8, 8, 8], &[]),
        ];
        for order in [[0, 1, 2], [1, 2, 0], [2, 0, 1], [1, 0, 2]] {
            let ordered: Vec<TestPage> = order
                .iter()
                .map(|&i| {
                    let p = &pages[i];
                    TestPage::new(p.width, p.height, p.photo, &p.bits, &p.extra)
                })
                .collect();
            let tiff = make_tiff_pages(&ordered);
            let info = probe_tiff(&mut Cursor::new(&tiff)).unwrap();
            assert_eq!(info.page_count, 3);
            assert_eq!(info.decoded_bytes, 10 * 10 * 8, "page order {order:?}");
            let produced = produced_page_bytes(&tiff);
            assert_eq!(info.decoded_bytes, produced.iter().copied().max().unwrap());

            let decoded = decode_tiff_pages(&mut Cursor::new(&tiff), true, |_, _| {}, || false)
                .unwrap()
                .unwrap();
            let per_page: Vec<u64> = decoded.iter().map(|(_, i)| i.decoded_bytes).collect();
            assert_eq!(per_page, produced, "page order {order:?}");
        }
    }

    fn make_minimal_tiff(
        photo: u16,
        bits_per_sample: &[u16],
        extra_sample: Option<u16>,
        sample_format: Option<u16>,
    ) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"II");
        bytes.extend_from_slice(&42u16.to_le_bytes());
        bytes.extend_from_slice(&8u32.to_le_bytes());

        let mut entries: Vec<(u16, u16, u32, Vec<u8>)> = Vec::new();
        entries.push((256, 3, 1, 10u16.to_le_bytes().to_vec()));
        entries.push((257, 3, 1, 10u16.to_le_bytes().to_vec()));
        if bits_per_sample.len() == 1 {
            entries.push((258, 3, 1, bits_per_sample[0].to_le_bytes().to_vec()));
        } else {
            let mut val_bytes = Vec::new();
            for &b in bits_per_sample {
                val_bytes.extend_from_slice(&b.to_le_bytes());
            }
            entries.push((258, 3, bits_per_sample.len() as u32, val_bytes));
        }
        entries.push((259, 3, 1, 1u16.to_le_bytes().to_vec()));
        entries.push((262, 3, 1, photo.to_le_bytes().to_vec()));
        entries.push((273, 4, 1, 500u32.to_le_bytes().to_vec()));
        entries.push((
            277,
            3,
            1,
            (bits_per_sample.len() as u16).to_le_bytes().to_vec(),
        ));
        entries.push((278, 3, 1, 10u16.to_le_bytes().to_vec()));
        entries.push((279, 4, 1, 100u32.to_le_bytes().to_vec()));

        if let Some(extra) = extra_sample {
            entries.push((338, 3, 1, extra.to_le_bytes().to_vec()));
        }
        if let Some(fmt) = sample_format {
            entries.push((339, 3, 1, fmt.to_le_bytes().to_vec()));
        }

        entries.sort_by_key(|e| e.0);

        let num_entries = entries.len() as u16;
        bytes.extend_from_slice(&num_entries.to_le_bytes());

        let ifd_len = 2 + entries.len() * 12 + 4;
        let overflow_offset = (8 + ifd_len) as u32;
        let mut overflow_data = Vec::new();

        for (tag, tag_type, count, val) in entries {
            bytes.extend_from_slice(&tag.to_le_bytes());
            bytes.extend_from_slice(&tag_type.to_le_bytes());
            bytes.extend_from_slice(&count.to_le_bytes());
            if val.len() <= 4 {
                let mut padded = [0u8; 4];
                padded[..val.len()].copy_from_slice(&val);
                bytes.extend_from_slice(&padded);
            } else {
                let off = overflow_offset + overflow_data.len() as u32;
                bytes.extend_from_slice(&off.to_le_bytes());
                overflow_data.extend_from_slice(&val);
            }
        }
        bytes.extend_from_slice(&0u32.to_le_bytes());
        bytes.extend_from_slice(&overflow_data);

        if bytes.len() < 600 {
            bytes.resize(600, 0);
        }
        bytes
    }

    #[test]
    fn pixel_expansion_1_2_4_bit_and_white_is_zero() {
        let raw = tiff::decoder::DecodingResult::U8(vec![0b10100000]);
        let (cs, bpc, data, alpha) =
            process_decoded_samples(8, 1, tiff::ColorType::Gray(1), 0, raw).unwrap();
        assert_eq!(cs, ImageColorspace::Luma);
        assert_eq!(bpc, BitsPerComponent::Eight);
        assert_eq!(alpha, None);
        assert_eq!(data, vec![255, 0, 255, 0, 0, 0, 0, 0]);

        let raw_2bit = tiff::decoder::DecodingResult::U8(vec![0b11100100]);
        let (cs, bpc, data, _) =
            process_decoded_samples(4, 1, tiff::ColorType::Gray(2), 0, raw_2bit).unwrap();
        assert_eq!(cs, ImageColorspace::Luma);
        assert_eq!(bpc, BitsPerComponent::Eight);
        assert_eq!(data, vec![255, 170, 85, 0]);

        let raw_4bit = tiff::decoder::DecodingResult::U8(vec![0b11110000]);
        let (cs, bpc, data, _) =
            process_decoded_samples(2, 1, tiff::ColorType::Gray(4), 0, raw_4bit).unwrap();
        assert_eq!(cs, ImageColorspace::Luma);
        assert_eq!(bpc, BitsPerComponent::Eight);
        assert_eq!(data, vec![255, 0]);
    }

    #[test]
    fn pixel_expansion_ycbcr_to_rgb() {
        let raw = tiff::decoder::DecodingResult::U8(vec![128, 128, 128]);
        let (cs, bpc, data, _) =
            process_decoded_samples(1, 1, tiff::ColorType::YCbCr(8), 0, raw).unwrap();
        assert_eq!(cs, ImageColorspace::Rgb);
        assert_eq!(bpc, BitsPerComponent::Eight);
        assert_eq!(data, vec![128, 128, 128]);

        let raw_white = tiff::decoder::DecodingResult::U8(vec![255, 128, 128]);
        let (_, _, data_white, _) =
            process_decoded_samples(1, 1, tiff::ColorType::YCbCr(8), 0, raw_white).unwrap();
        assert_eq!(data_white, vec![255, 255, 255]);

        let raw_black = tiff::decoder::DecodingResult::U8(vec![0, 128, 128]);
        let (_, _, data_black, _) =
            process_decoded_samples(1, 1, tiff::ColorType::YCbCr(8), 0, raw_black).unwrap();
        assert_eq!(data_black, vec![0, 0, 0]);
    }

    #[test]
    fn pixel_expansion_associated_and_unassociated_alpha() {
        let raw_assoc = tiff::decoder::DecodingResult::U8(vec![64, 64, 64, 128, 0, 0, 0, 0]);
        let (cs, bpc, color, alpha) =
            process_decoded_samples(2, 1, tiff::ColorType::RGBA(8), 1, raw_assoc).unwrap();
        assert_eq!(cs, ImageColorspace::Rgb);
        assert_eq!(bpc, BitsPerComponent::Eight);
        assert_eq!(color, vec![128, 128, 128, 0, 0, 0]);
        assert_eq!(alpha, Some(vec![128, 0]));

        let raw_unassoc = tiff::decoder::DecodingResult::U8(vec![64, 64, 64, 128]);
        let (_, _, color_unassoc, alpha_unassoc) =
            process_decoded_samples(1, 1, tiff::ColorType::RGBA(8), 2, raw_unassoc).unwrap();
        assert_eq!(color_unassoc, vec![64, 64, 64]);
        assert_eq!(alpha_unassoc, Some(vec![128]));

        let raw_unspec = tiff::decoder::DecodingResult::U8(vec![64, 64, 64]);
        let (_, _, color_unspec, alpha_unspec) =
            process_decoded_samples(1, 1, tiff::ColorType::RGB(8), 0, raw_unspec).unwrap();
        assert_eq!(color_unspec, vec![64, 64, 64]);
        assert_eq!(alpha_unspec, None);
    }

    #[test]
    fn pixel_expansion_16bit_byte_order() {
        let raw_u16 = tiff::decoder::DecodingResult::U16(vec![0x1234]);
        let (cs, bpc, data, _) =
            process_decoded_samples(1, 1, tiff::ColorType::Gray(16), 0, raw_u16).unwrap();
        assert_eq!(cs, ImageColorspace::Luma);
        assert_eq!(bpc, BitsPerComponent::Sixteen);
        assert_eq!(data, vec![0x12, 0x34]);

        let raw_rgb16 = tiff::decoder::DecodingResult::U16(vec![0x0102, 0x0304, 0x0506]);
        let (cs, bpc, rgb_data, _) =
            process_decoded_samples(1, 1, tiff::ColorType::RGB(16), 0, raw_rgb16).unwrap();
        assert_eq!(cs, ImageColorspace::Rgb);
        assert_eq!(bpc, BitsPerComponent::Sixteen);
        assert_eq!(rgb_data, vec![0x01, 0x02, 0x03, 0x04, 0x05, 0x06]);
    }

    #[test]
    fn extra_samples_unspecified_and_multiband_conversion() {
        // 1. 4-sample RGB with ExtraSamples [0] -> Decodes to opaque RGB(8).
        let tiff_rgb_unspec = make_test_tiff_image(
            1,
            1,
            2,
            &[8, 8, 8, 8],
            &[0],
            &[255, 128, 64, 10], // 4th sample is unspecified extra data
        );
        let pages = decode_tiff_pages(
            &mut Cursor::new(&tiff_rgb_unspec),
            true,
            &mut |_, _| {},
            || false,
        )
        .expect("decoding should succeed")
        .expect("not cancelled");
        assert_eq!(pages.len(), 1);
        let (_, info) = &pages[0];
        assert_eq!(info.width, 1);
        assert_eq!(info.height, 1);

        let dyn_img1 = decode_tiff_first_page_as_image(&tiff_rgb_unspec).unwrap();
        assert_eq!(dyn_img1.color(), image::ColorType::Rgb8);

        // 2. 5-sample RGB with ExtraSamples [2, 0] -> Decodes to RGBA(8) with 1st alpha sample.
        let tiff_rgba_unassoc = make_test_tiff_image(
            1,
            1,
            2,
            &[8, 8, 8, 8, 8],
            &[2, 0],
            &[255, 128, 64, 200, 10], // 4th is alpha, 5th is extra data
        );
        let pages2 = decode_tiff_pages(
            &mut Cursor::new(&tiff_rgba_unassoc),
            true,
            &mut |_, _| {},
            || false,
        )
        .expect("decoding should succeed")
        .expect("not cancelled");
        assert_eq!(pages2.len(), 1);

        let dyn_img2 = decode_tiff_first_page_as_image(&tiff_rgba_unassoc).unwrap();
        assert_eq!(dyn_img2.color(), image::ColorType::Rgba8);
        assert_eq!(dyn_img2.to_rgba8().into_raw(), vec![255, 128, 64, 200]);

        // 3. 2-sample Gray with ExtraSamples [0] -> Decodes to opaque Gray(8).
        let tiff_gray_unspec = make_test_tiff_image(
            1,
            1,
            1,
            &[8, 8],
            &[0],
            &[180, 50], // 2nd sample is unspecified extra data
        );
        let pages3 = decode_tiff_pages(
            &mut Cursor::new(&tiff_gray_unspec),
            true,
            &mut |_, _| {},
            || false,
        )
        .expect("decoding should succeed")
        .expect("not cancelled");
        assert_eq!(pages3.len(), 1);

        let dyn_img3 = decode_tiff_first_page_as_image(&tiff_gray_unspec).unwrap();
        assert_eq!(dyn_img3.color(), image::ColorType::L8);
        assert_eq!(dyn_img3.to_luma8().into_raw(), vec![180]);
    }

    #[test]
    fn probe_detects_unsupported_format_on_page2() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"II");
        bytes.extend_from_slice(&42u16.to_le_bytes());
        bytes.extend_from_slice(&8u32.to_le_bytes()); // IFD 1 offset = 8

        let ifd1_entries = 6u16;
        let ifd2_offset = (8 + 2 + ifd1_entries * 12 + 4) as u32;

        // Page 1: valid 10x10 Gray (photo = 1)
        bytes.extend_from_slice(&ifd1_entries.to_le_bytes());
        bytes.extend_from_slice(&256u16.to_le_bytes());
        bytes.extend_from_slice(&3u16.to_le_bytes());
        bytes.extend_from_slice(&1u32.to_le_bytes());
        bytes.extend_from_slice(&10u16.to_le_bytes());
        bytes.extend_from_slice(&[0, 0]);

        bytes.extend_from_slice(&257u16.to_le_bytes());
        bytes.extend_from_slice(&3u16.to_le_bytes());
        bytes.extend_from_slice(&1u32.to_le_bytes());
        bytes.extend_from_slice(&10u16.to_le_bytes());
        bytes.extend_from_slice(&[0, 0]);

        bytes.extend_from_slice(&259u16.to_le_bytes());
        bytes.extend_from_slice(&3u16.to_le_bytes());
        bytes.extend_from_slice(&1u32.to_le_bytes());
        bytes.extend_from_slice(&1u16.to_le_bytes());
        bytes.extend_from_slice(&[0, 0]);

        bytes.extend_from_slice(&262u16.to_le_bytes());
        bytes.extend_from_slice(&3u16.to_le_bytes());
        bytes.extend_from_slice(&1u32.to_le_bytes());
        bytes.extend_from_slice(&1u16.to_le_bytes());
        bytes.extend_from_slice(&[0, 0]);

        bytes.extend_from_slice(&273u16.to_le_bytes());
        bytes.extend_from_slice(&4u16.to_le_bytes());
        bytes.extend_from_slice(&1u32.to_le_bytes());
        bytes.extend_from_slice(&2000u32.to_le_bytes());

        bytes.extend_from_slice(&279u16.to_le_bytes());
        bytes.extend_from_slice(&4u16.to_le_bytes());
        bytes.extend_from_slice(&1u32.to_le_bytes());
        bytes.extend_from_slice(&100u32.to_le_bytes());

        bytes.extend_from_slice(&ifd2_offset.to_le_bytes());

        // Page 2: unsupported CMYK (photo = 5)
        let ifd2_entries = 6u16;
        bytes.extend_from_slice(&ifd2_entries.to_le_bytes());
        bytes.extend_from_slice(&256u16.to_le_bytes());
        bytes.extend_from_slice(&3u16.to_le_bytes());
        bytes.extend_from_slice(&1u32.to_le_bytes());
        bytes.extend_from_slice(&10u16.to_le_bytes());
        bytes.extend_from_slice(&[0, 0]);

        bytes.extend_from_slice(&257u16.to_le_bytes());
        bytes.extend_from_slice(&3u16.to_le_bytes());
        bytes.extend_from_slice(&1u32.to_le_bytes());
        bytes.extend_from_slice(&10u16.to_le_bytes());
        bytes.extend_from_slice(&[0, 0]);

        bytes.extend_from_slice(&259u16.to_le_bytes());
        bytes.extend_from_slice(&3u16.to_le_bytes());
        bytes.extend_from_slice(&1u32.to_le_bytes());
        bytes.extend_from_slice(&1u16.to_le_bytes());
        bytes.extend_from_slice(&[0, 0]);

        bytes.extend_from_slice(&262u16.to_le_bytes());
        bytes.extend_from_slice(&3u16.to_le_bytes());
        bytes.extend_from_slice(&1u32.to_le_bytes());
        bytes.extend_from_slice(&5u16.to_le_bytes()); // CMYK
        bytes.extend_from_slice(&[0, 0]);

        bytes.extend_from_slice(&273u16.to_le_bytes());
        bytes.extend_from_slice(&4u16.to_le_bytes());
        bytes.extend_from_slice(&1u32.to_le_bytes());
        bytes.extend_from_slice(&2000u32.to_le_bytes());

        bytes.extend_from_slice(&279u16.to_le_bytes());
        bytes.extend_from_slice(&4u16.to_le_bytes());
        bytes.extend_from_slice(&1u32.to_le_bytes());
        bytes.extend_from_slice(&100u32.to_le_bytes());

        bytes.extend_from_slice(&0u32.to_le_bytes()); // next IFD = 0

        let mut cursor = Cursor::new(bytes);
        assert_eq!(probe_tiff(&mut cursor), Err(ProbeError::UnsupportedFormat));
    }

    #[test]
    fn probe_accepts_exactly_max_tiff_pages_and_rejects_one_more() {
        let tiff_1000 = make_multi_ifd_tiff(MAX_TIFF_PAGES);
        let mut cursor_1000 = Cursor::new(tiff_1000);
        let info = probe_tiff(&mut cursor_1000).expect("MAX_TIFF_PAGES must be accepted");
        assert_eq!(info.page_count, MAX_TIFF_PAGES);

        let tiff_1001 = make_multi_ifd_tiff(MAX_TIFF_PAGES + 1);
        let mut cursor_1001 = Cursor::new(tiff_1001);
        assert_eq!(probe_tiff(&mut cursor_1001), Err(ProbeError::TooManyPages));
    }

    fn make_multi_ifd_tiff(num_pages: u32) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"II");
        bytes.extend_from_slice(&42u16.to_le_bytes());
        bytes.extend_from_slice(&8u32.to_le_bytes());

        for i in 0..num_pages {
            let is_last = i + 1 == num_pages;
            let num_entries: u16 = 6;
            bytes.extend_from_slice(&num_entries.to_le_bytes());

            bytes.extend_from_slice(&256u16.to_le_bytes());
            bytes.extend_from_slice(&3u16.to_le_bytes());
            bytes.extend_from_slice(&1u32.to_le_bytes());
            bytes.extend_from_slice(&10u16.to_le_bytes());
            bytes.extend_from_slice(&[0, 0]);

            bytes.extend_from_slice(&257u16.to_le_bytes());
            bytes.extend_from_slice(&3u16.to_le_bytes());
            bytes.extend_from_slice(&1u32.to_le_bytes());
            bytes.extend_from_slice(&10u16.to_le_bytes());
            bytes.extend_from_slice(&[0, 0]);

            bytes.extend_from_slice(&259u16.to_le_bytes());
            bytes.extend_from_slice(&3u16.to_le_bytes());
            bytes.extend_from_slice(&1u32.to_le_bytes());
            bytes.extend_from_slice(&1u16.to_le_bytes());
            bytes.extend_from_slice(&[0, 0]);

            bytes.extend_from_slice(&262u16.to_le_bytes());
            bytes.extend_from_slice(&3u16.to_le_bytes());
            bytes.extend_from_slice(&1u32.to_le_bytes());
            bytes.extend_from_slice(&1u16.to_le_bytes());
            bytes.extend_from_slice(&[0, 0]);

            bytes.extend_from_slice(&273u16.to_le_bytes());
            bytes.extend_from_slice(&4u16.to_le_bytes());
            bytes.extend_from_slice(&1u32.to_le_bytes());
            bytes.extend_from_slice(&100000u32.to_le_bytes());

            bytes.extend_from_slice(&279u16.to_le_bytes());
            bytes.extend_from_slice(&4u16.to_le_bytes());
            bytes.extend_from_slice(&1u32.to_le_bytes());
            bytes.extend_from_slice(&100u32.to_le_bytes());

            let next_offset = if is_last {
                0u32
            } else {
                (bytes.len() + 4) as u32
            };
            bytes.extend_from_slice(&next_offset.to_le_bytes());
        }
        bytes
    }

    fn make_test_tiff_image(
        width: u32,
        height: u32,
        photo: u16,
        bits_per_sample: &[u16],
        extra_samples: &[u16],
        pixels: &[u8],
    ) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"II");
        bytes.extend_from_slice(&42u16.to_le_bytes());
        bytes.extend_from_slice(&8u32.to_le_bytes());

        let mut entries: Vec<(u16, u16, u32, Vec<u8>)> = Vec::new();
        entries.push((256, 4, 1, width.to_le_bytes().to_vec()));
        entries.push((257, 4, 1, height.to_le_bytes().to_vec()));
        if bits_per_sample.len() == 1 {
            entries.push((258, 3, 1, bits_per_sample[0].to_le_bytes().to_vec()));
        } else {
            let mut val_bytes = Vec::new();
            for &b in bits_per_sample {
                val_bytes.extend_from_slice(&b.to_le_bytes());
            }
            entries.push((258, 3, bits_per_sample.len() as u32, val_bytes));
        }
        entries.push((259, 3, 1, 1u16.to_le_bytes().to_vec()));
        entries.push((262, 3, 1, photo.to_le_bytes().to_vec()));
        entries.push((273, 4, 1, 0u32.to_le_bytes().to_vec()));
        entries.push((
            277,
            3,
            1,
            (bits_per_sample.len() as u16).to_le_bytes().to_vec(),
        ));
        entries.push((278, 4, 1, height.to_le_bytes().to_vec()));
        entries.push((279, 4, 1, (pixels.len() as u32).to_le_bytes().to_vec()));

        if !extra_samples.is_empty() {
            if extra_samples.len() == 1 {
                entries.push((338, 3, 1, extra_samples[0].to_le_bytes().to_vec()));
            } else {
                let mut val_bytes = Vec::new();
                for &es in extra_samples {
                    val_bytes.extend_from_slice(&es.to_le_bytes());
                }
                entries.push((338, 3, extra_samples.len() as u32, val_bytes));
            }
        }

        entries.sort_by_key(|e| e.0);

        let num_entries = entries.len() as u16;
        bytes.extend_from_slice(&num_entries.to_le_bytes());

        let ifd_len = 2 + entries.len() * 12 + 4;
        let overflow_offset = (8 + ifd_len) as u32;
        let mut overflow_data = Vec::new();

        for entry in &mut entries {
            if entry.3.len() > 4 {
                let off = overflow_offset + overflow_data.len() as u32;
                overflow_data.extend_from_slice(&entry.3);
                entry.3 = off.to_le_bytes().to_vec();
            }
        }

        let strip_offset = overflow_offset + overflow_data.len() as u32;
        for entry in &mut entries {
            if entry.0 == 273 {
                entry.3 = strip_offset.to_le_bytes().to_vec();
            }
        }

        for (tag, tag_type, count, val) in entries {
            bytes.extend_from_slice(&tag.to_le_bytes());
            bytes.extend_from_slice(&tag_type.to_le_bytes());
            bytes.extend_from_slice(&count.to_le_bytes());
            let mut padded = [0u8; 4];
            padded[..val.len().min(4)].copy_from_slice(&val[..val.len().min(4)]);
            bytes.extend_from_slice(&padded);
        }
        bytes.extend_from_slice(&0u32.to_le_bytes());
        bytes.extend_from_slice(&overflow_data);
        bytes.extend_from_slice(pixels);
        bytes
    }
}
