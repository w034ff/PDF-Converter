//! Output filename resolution for batch conversion (design §6.3, §6.4).

use std::collections::HashSet;

/// Splits a filename into its stem and extension based on the last period.
///
/// For multi-part extensions like `archive.tar.gz`, only the final extension is split off:
/// stem is `archive.tar`, extension is `gz`.
/// If there is no period, stem is `filename` and extension is empty.
#[must_use]
pub fn split_stem_and_ext(filename: &str) -> (&str, &str) {
    match filename.rfind('.') {
        Some(idx) => (&filename[..idx], &filename[idx + 1..]),
        None => (filename, ""),
    }
}

/// Returns the zero-padded width for page numbers based on the total page count (design §6.3).
///
/// For example:
/// - 1..=9 pages -> 1 digit (`p1` .. `p9`)
/// - 10..=99 pages -> 2 digits (`p01` .. `p10`)
/// - 100..=999 pages -> 3 digits (`p001` .. `p120`)
#[must_use]
pub fn page_number_width(total_pages: u32) -> usize {
    total_pages.max(1).to_string().len()
}

/// Formats the base output name for a single PDF page before collision resolution (design §6.3).
///
/// - `pdf_filename`: Name of the source PDF (e.g. `report.pdf` or `archive.tar.pdf`).
/// - `page_number`: 1-based page number.
/// - `total_pages`: Total page count of the source PDF.
/// - `extension`: Output image extension without leading dot (e.g. `png`, `jpg`).
///
/// E.g. `format_pdf_page_name("report.pdf", 1, 120, "png")` -> `"report_p001.png"`.
/// E.g. `format_pdf_page_name("archive.tar.pdf", 9, 9, "jpg")` -> `"archive.tar_p9.jpg"`.
#[must_use]
pub fn format_pdf_page_name(
    pdf_filename: &str,
    page_number: u32,
    total_pages: u32,
    extension: &str,
) -> String {
    let (stem, _) = split_stem_and_ext(pdf_filename);
    let width = page_number_width(total_pages);
    let ext = extension.trim_start_matches('.');
    if ext.is_empty() {
        format!("{stem}_p{page_number:0width$}")
    } else {
        format!("{stem}_p{page_number:0width$}.{ext}")
    }
}

/// Extracts any existing trailing ` (n)` suffix from a stem.
///
/// If `stem` ends with ` (n)` where `n >= 1`, returns `(base_stem, Some(n))`.
/// Otherwise returns `(stem, None)`.
fn split_numeric_suffix(stem: &str) -> (&str, Option<usize>) {
    if let Some(open_idx) = stem.rfind(" (")
        && stem.ends_with(')')
    {
        let num_str = &stem[open_idx + 2..stem.len() - 1];
        if let Ok(num) = num_str.parse::<usize>()
            && num > 0
        {
            return (&stem[..open_idx], Some(num));
        }
    }
    (stem, None)
}

/// Finds the next available filename using the `name (n).ext` scheme
/// if `target_name` is taken according to `is_taken` (design §6.4).
///
/// If `target_name` is not taken, returns `target_name` unchanged.
/// If `target_name` already has a ` (k)` suffix and is taken, increments from `k + 1`.
pub fn next_available_name<F>(target_name: &str, is_taken: F) -> String
where
    F: Fn(&str) -> bool,
{
    if !is_taken(target_name) {
        return target_name.to_string();
    }

    let (stem, ext) = split_stem_and_ext(target_name);
    let (base_stem, start_n) = split_numeric_suffix(stem);
    let mut n = start_n.map_or(1, |cur| cur + 1);

    loop {
        let candidate = if ext.is_empty() {
            format!("{base_stem} ({n})")
        } else {
            format!("{base_stem} ({n}).{ext}")
        };
        if !is_taken(&candidate) {
            return candidate;
        }
        n += 1;
    }
}

/// Resolves unique output PDF filenames for individual image conversion (design §6.2, §6.4).
///
/// Each output filename corresponds to the input filename at the same index in `inputs`.
/// The base name replaces only the final extension with `.pdf` (e.g., `a.b.png` -> `a.b.pdf`).
/// Names are assigned in Unicode code point order of the input filenames so that results
/// are invariant to the input order.
/// If a candidate name collides with an existing file in `existing` or an already assigned
/// output name, an incrementing numeric suffix is appended: `name (1).pdf`, `name (2).pdf`, etc.
/// Collision checks are case-insensitive, while returned output names preserve the original
/// casing of the base name.
pub fn resolve_image_to_pdf_names(inputs: &[String], existing: &HashSet<String>) -> Vec<String> {
    // Pair each input with its original index so results can be restored to input order.
    let mut indexed_inputs: Vec<(&str, usize)> = inputs
        .iter()
        .enumerate()
        .map(|(i, s)| (s.as_str(), i))
        .collect();

    // Sort by input filename in Unicode code point order (UTF-8 byte comparison).
    // Stable sort ensures predictable assignment order even if identical inputs are passed.
    indexed_inputs.sort_by(|a, b| a.0.cmp(b.0));

    // Track all lowercased names that are already occupied (existing files and assigned outputs).
    let mut used_lower: HashSet<String> = existing.iter().map(|s| s.to_lowercase()).collect();
    let mut resolved = vec![String::new(); inputs.len()];

    for (input, original_index) in indexed_inputs {
        let (stem, _) = split_stem_and_ext(input);
        let mut n = 0usize;
        loop {
            let candidate = if n == 0 {
                format!("{stem}.pdf")
            } else {
                format!("{stem} ({n}).pdf")
            };

            let candidate_lower = candidate.to_lowercase();
            if !used_lower.contains(&candidate_lower) {
                used_lower.insert(candidate_lower);
                resolved[original_index] = candidate;
                break;
            }
            n += 1;
        }
    }

    resolved
}

/// Input specification for a PDF and its selected pages to be converted to images.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PdfToImageInput {
    /// Filename of the PDF (e.g. `report.pdf`).
    pub filename: String,
    /// Total number of pages in the PDF, used to calculate zero-padding width.
    pub total_pages: u32,
    /// 1-based page numbers selected for conversion.
    pub pages: Vec<u32>,
}

/// Resolves output image filenames for multiple PDF documents and their selected pages (design §6.3, §6.4).
///
/// Returns a `Vec<Vec<String>>` where each inner vector corresponds to the `pages` of the [`PdfToImageInput`]
/// at the same index, and contains the resolved filenames in the same order as `input.pages`.
///
/// Names are assigned in Unicode code point order of `(filename, page_number)` so the result
/// is invariant to the input order of documents and pages.
/// Collision checks against `existing` and previously assigned outputs are case-insensitive.
pub fn resolve_pdf_to_image_names(
    inputs: &[PdfToImageInput],
    extension: &str,
    existing: &HashSet<String>,
) -> Vec<Vec<String>> {
    let ext = extension.trim_start_matches('.');

    struct PageEntry<'a> {
        input_idx: usize,
        page_idx: usize,
        filename: &'a str,
        page: u32,
        total_pages: u32,
    }

    let mut entries = Vec::new();
    let mut result: Vec<Vec<String>> = inputs
        .iter()
        .map(|input| vec![String::new(); input.pages.len()])
        .collect();

    for (input_idx, input) in inputs.iter().enumerate() {
        for (page_idx, &page) in input.pages.iter().enumerate() {
            entries.push(PageEntry {
                input_idx,
                page_idx,
                filename: input.filename.as_str(),
                page,
                total_pages: input.total_pages,
            });
        }
    }

    // Sort entries by (filename, page_number) in Unicode code point order.
    // Stable sort ensures predictable ordering even if identical files/pages are present.
    entries.sort_by(|a, b| (a.filename, a.page).cmp(&(b.filename, b.page)));

    let mut used_lower: HashSet<String> = existing.iter().map(|s| s.to_lowercase()).collect();

    for entry in entries {
        let (stem, _) = split_stem_and_ext(entry.filename);
        let width = page_number_width(entry.total_pages);
        let base_stem = format!("{stem}_p{:0width$}", entry.page, width = width);

        let mut n = 0usize;
        loop {
            let candidate = if n == 0 {
                if ext.is_empty() {
                    base_stem.clone()
                } else {
                    format!("{base_stem}.{ext}")
                }
            } else if ext.is_empty() {
                format!("{base_stem} ({n})")
            } else {
                format!("{base_stem} ({n}).{ext}")
            };

            let candidate_lower = candidate.to_lowercase();
            if !used_lower.contains(&candidate_lower) {
                used_lower.insert(candidate_lower);
                result[entry.input_idx][entry.page_idx] = candidate;
                break;
            }
            n += 1;
        }
    }

    result
}

/// An item representing a single PDF page to convert to an image.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PdfPageItem {
    /// Filename of the PDF.
    pub pdf_filename: String,
    /// 1-based page number.
    pub page_number: u32,
    /// Total page count of the PDF.
    pub total_pages: u32,
}

/// Resolves output image filenames for a flat list of PDF page items (design §6.3, §6.4).
///
/// Output filenames correspond to `items` at the same index.
/// Assignment order is sorted by `(pdf_filename, page_number)`.
pub fn resolve_pdf_page_output_names(
    items: &[PdfPageItem],
    extension: &str,
    existing: &HashSet<String>,
) -> Vec<String> {
    let ext = extension.trim_start_matches('.');
    let mut indexed: Vec<(&PdfPageItem, usize)> =
        items.iter().enumerate().map(|(i, it)| (it, i)).collect();
    indexed.sort_by(|a, b| {
        (a.0.pdf_filename.as_str(), a.0.page_number)
            .cmp(&(b.0.pdf_filename.as_str(), b.0.page_number))
    });

    let mut used_lower: HashSet<String> = existing.iter().map(|s| s.to_lowercase()).collect();
    let mut resolved = vec![String::new(); items.len()];

    for (item, orig_idx) in indexed {
        let (stem, _) = split_stem_and_ext(&item.pdf_filename);
        let width = page_number_width(item.total_pages);
        let base_stem = format!("{stem}_p{:0width$}", item.page_number, width = width);

        let mut n = 0usize;
        loop {
            let candidate = if n == 0 {
                if ext.is_empty() {
                    base_stem.clone()
                } else {
                    format!("{base_stem}.{ext}")
                }
            } else if ext.is_empty() {
                format!("{base_stem} ({n})")
            } else {
                format!("{base_stem} ({n}).{ext}")
            };

            let candidate_lower = candidate.to_lowercase();
            if !used_lower.contains(&candidate_lower) {
                used_lower.insert(candidate_lower);
                resolved[orig_idx] = candidate;
                break;
            }
            n += 1;
        }
    }

    resolved
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_collision() {
        let inputs = vec!["doc1.png".to_string(), "doc2.jpg".to_string()];
        let existing = HashSet::new();
        let result = resolve_image_to_pdf_names(&inputs, &existing);
        assert_eq!(result, vec!["doc1.pdf", "doc2.pdf"]);

        let pdf_inputs = vec![PdfToImageInput {
            filename: "doc.pdf".to_string(),
            total_pages: 5,
            pages: vec![1, 2],
        }];
        let pdf_result = resolve_pdf_to_image_names(&pdf_inputs, "png", &existing);
        assert_eq!(pdf_result, vec![vec!["doc_p1.png", "doc_p2.png"]]);
    }

    #[test]
    fn collision_with_existing_files() {
        let inputs = vec!["doc.png".to_string()];
        let mut existing = HashSet::new();
        existing.insert("doc.pdf".to_string());
        let result = resolve_image_to_pdf_names(&inputs, &existing);
        assert_eq!(result, vec!["doc (1).pdf"]);

        let pdf_inputs = vec![PdfToImageInput {
            filename: "report.pdf".to_string(),
            total_pages: 5,
            pages: vec![1],
        }];
        let mut pdf_existing = HashSet::new();
        pdf_existing.insert("report_p1.png".to_string());
        let pdf_result = resolve_pdf_to_image_names(&pdf_inputs, "png", &pdf_existing);
        assert_eq!(pdf_result, vec![vec!["report_p1 (1).png"]]);
    }

    #[test]
    fn collision_between_inputs() {
        // Two inputs that map to the same base PDF filename.
        let inputs = vec!["doc.png".to_string(), "doc.png".to_string()];
        let existing = HashSet::new();
        let result = resolve_image_to_pdf_names(&inputs, &existing);
        assert_eq!(result, vec!["doc.pdf", "doc (1).pdf"]);

        // Different extensions that both map to doc.pdf.
        // Alphabetically "doc.jpg" < "doc.png", so "doc.jpg" gets "doc.pdf" and "doc.png" gets "doc (1).pdf".
        let inputs_diff = vec!["doc.png".to_string(), "doc.jpg".to_string()];
        let result_diff = resolve_image_to_pdf_names(&inputs_diff, &existing);
        assert_eq!(result_diff, vec!["doc (1).pdf", "doc.pdf"]);

        // PDF to images with identical PDF names (e.g. from different folders).
        let pdf_inputs = vec![
            PdfToImageInput {
                filename: "report.pdf".to_string(),
                total_pages: 5,
                pages: vec![1],
            },
            PdfToImageInput {
                filename: "report.pdf".to_string(),
                total_pages: 5,
                pages: vec![1],
            },
        ];
        let pdf_result = resolve_pdf_to_image_names(&pdf_inputs, "png", &existing);
        assert_eq!(
            pdf_result,
            vec![vec!["report_p1.png"], vec!["report_p1 (1).png"]]
        );
    }

    #[test]
    fn numeric_suffix_rollover() {
        let inputs = vec!["doc.png".to_string()];
        let mut existing = HashSet::new();
        existing.insert("doc.pdf".to_string());
        existing.insert("doc (1).pdf".to_string());
        existing.insert("doc (2).pdf".to_string());
        let result = resolve_image_to_pdf_names(&inputs, &existing);
        assert_eq!(result, vec!["doc (3).pdf"]);

        // Inputs colliding with existing numbers:
        let inputs_multi = vec!["doc.png".to_string(), "doc.jpg".to_string()];
        let mut existing_multi = HashSet::new();
        existing_multi.insert("doc.pdf".to_string());
        existing_multi.insert("doc (1).pdf".to_string());
        let result_multi = resolve_image_to_pdf_names(&inputs_multi, &existing_multi);
        // "doc.jpg" < "doc.png", so doc.jpg gets (2), doc.png gets (3)
        assert_eq!(result_multi, vec!["doc (3).pdf", "doc (2).pdf"]);
    }

    #[test]
    fn case_insensitivity() {
        // Collision check ignores case, but returns candidate with original base casing.
        let inputs = vec!["doc.png".to_string()];
        let mut existing = HashSet::new();
        existing.insert("DOC.PDF".to_string());
        let result = resolve_image_to_pdf_names(&inputs, &existing);
        assert_eq!(result, vec!["doc (1).pdf"]);

        // Input casing collision:
        let inputs_case = vec!["doc.png".to_string(), "DOC.png".to_string()];
        let existing_empty = HashSet::new();
        let result_case = resolve_image_to_pdf_names(&inputs_case, &existing_empty);
        // In Unicode code point order, 'D' (0x44) < 'd' (0x64), so "DOC.png" comes first.
        // "DOC.png" gets "DOC.pdf", and "doc.png" collides case-insensitively, getting "doc (1).pdf".
        assert_eq!(result_case, vec!["doc (1).pdf", "DOC.pdf"]);
    }

    #[test]
    fn multi_part_extension() {
        let inputs = vec!["archive.tar.png".to_string()];
        let existing = HashSet::new();
        let result = resolve_image_to_pdf_names(&inputs, &existing);
        assert_eq!(result, vec!["archive.tar.pdf"]);

        let mut existing_coll = HashSet::new();
        existing_coll.insert("archive.tar.pdf".to_string());
        let result_coll = resolve_image_to_pdf_names(&inputs, &existing_coll);
        assert_eq!(result_coll, vec!["archive.tar (1).pdf"]);

        let pdf_inputs = vec![PdfToImageInput {
            filename: "archive.tar.pdf".to_string(),
            total_pages: 5,
            pages: vec![2],
        }];
        let pdf_result = resolve_pdf_to_image_names(&pdf_inputs, "jpg", &existing);
        assert_eq!(pdf_result, vec![vec!["archive.tar_p2.jpg"]]);
    }

    #[test]
    fn pdf_page_number_digits_boundary_and_large() {
        // 9 pages vs 10 pages boundary
        assert_eq!(page_number_width(9), 1);
        assert_eq!(format_pdf_page_name("doc.pdf", 1, 9, "png"), "doc_p1.png");
        assert_eq!(format_pdf_page_name("doc.pdf", 9, 9, "png"), "doc_p9.png");

        assert_eq!(page_number_width(10), 2);
        assert_eq!(format_pdf_page_name("doc.pdf", 1, 10, "png"), "doc_p01.png");
        assert_eq!(format_pdf_page_name("doc.pdf", 9, 10, "png"), "doc_p09.png");
        assert_eq!(
            format_pdf_page_name("doc.pdf", 10, 10, "png"),
            "doc_p10.png"
        );

        // 120 pages
        assert_eq!(page_number_width(120), 3);
        assert_eq!(
            format_pdf_page_name("doc.pdf", 1, 120, "png"),
            "doc_p001.png"
        );
        assert_eq!(
            format_pdf_page_name("doc.pdf", 10, 120, "png"),
            "doc_p010.png"
        );
        assert_eq!(
            format_pdf_page_name("doc.pdf", 120, 120, "png"),
            "doc_p120.png"
        );
    }

    #[test]
    fn invariant_under_input_order() {
        let mut existing = HashSet::new();
        existing.insert("alpha.pdf".to_string());

        let inputs_a = vec!["beta.png".to_string(), "alpha.png".to_string()];
        let result_a = resolve_image_to_pdf_names(&inputs_a, &existing);
        // beta.png is at idx 0, alpha.png is at idx 1
        assert_eq!(result_a, vec!["beta.pdf", "alpha (1).pdf"]);

        let inputs_b = vec!["alpha.png".to_string(), "beta.png".to_string()];
        let result_b = resolve_image_to_pdf_names(&inputs_b, &existing);
        // alpha.png is at idx 0, beta.png is at idx 1
        assert_eq!(result_b, vec!["alpha (1).pdf", "beta.pdf"]);

        // Input collisions invariance
        let inputs_1 = vec!["doc.png".to_string(), "doc.jpg".to_string()];
        let result_1 = resolve_image_to_pdf_names(&inputs_1, &HashSet::new());
        // doc.jpg is at idx 1 -> "doc.pdf", doc.png is at idx 0 -> "doc (1).pdf"
        assert_eq!(result_1, vec!["doc (1).pdf", "doc.pdf"]);

        let inputs_2 = vec!["doc.jpg".to_string(), "doc.png".to_string()];
        let result_2 = resolve_image_to_pdf_names(&inputs_2, &HashSet::new());
        // doc.jpg is at idx 0 -> "doc.pdf", doc.png is at idx 1 -> "doc (1).pdf"
        assert_eq!(result_2, vec!["doc.pdf", "doc (1).pdf"]);

        // PDF to images invariance
        let pdf_a = PdfToImageInput {
            filename: "b.pdf".to_string(),
            total_pages: 5,
            pages: vec![1, 2],
        };
        let pdf_b = PdfToImageInput {
            filename: "a.pdf".to_string(),
            total_pages: 5,
            pages: vec![1, 2],
        };
        let mut pdf_existing = HashSet::new();
        pdf_existing.insert("a_p1.png".to_string());

        let order_1 =
            resolve_pdf_to_image_names(&[pdf_a.clone(), pdf_b.clone()], "png", &pdf_existing);
        let order_2 = resolve_pdf_to_image_names(&[pdf_b, pdf_a], "png", &pdf_existing);

        // In order_1, input 0 is b.pdf, input 1 is a.pdf.
        // In order_2, input 0 is a.pdf, input 1 is b.pdf.
        assert_eq!(order_1[0], order_2[1]); // b.pdf results match
        assert_eq!(order_1[1], order_2[0]); // a.pdf results match
        assert_eq!(order_1[1], vec!["a_p1 (1).png", "a_p2.png"]);
        assert_eq!(order_1[0], vec!["b_p1.png", "b_p2.png"]);
    }

    #[test]
    fn flat_pdf_page_items_resolution() {
        let items = vec![
            PdfPageItem {
                pdf_filename: "doc.pdf".to_string(),
                page_number: 2,
                total_pages: 10,
            },
            PdfPageItem {
                pdf_filename: "doc.pdf".to_string(),
                page_number: 1,
                total_pages: 10,
            },
        ];
        let mut existing = HashSet::new();
        existing.insert("doc_p01.png".to_string());
        let result = resolve_pdf_page_output_names(&items, "png", &existing);
        assert_eq!(result, vec!["doc_p02.png", "doc_p01 (1).png"]);
    }

    #[test]
    fn next_available_name_tests() {
        let mut taken = HashSet::new();
        taken.insert("output.pdf".to_string());
        taken.insert("output (1).pdf".to_string());
        taken.insert("readme".to_string());
        taken.insert("case.pdf".to_string());

        let is_taken = |name: &str| taken.contains(&name.to_lowercase());

        // Not taken
        assert_eq!(next_available_name("fresh.pdf", is_taken), "fresh.pdf");

        // Taken -> (2) because (1) is also taken
        assert_eq!(
            next_available_name("output.pdf", is_taken),
            "output (2).pdf"
        );

        // Already has (1), rolls over to (2)
        assert_eq!(
            next_available_name("output (1).pdf", is_taken),
            "output (2).pdf"
        );

        // Case-insensitivity
        assert_eq!(next_available_name("CASE.pdf", is_taken), "CASE (1).pdf");

        // No extension
        assert_eq!(next_available_name("readme", is_taken), "readme (1)");
    }
}
