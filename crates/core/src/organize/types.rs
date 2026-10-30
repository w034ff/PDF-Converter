//! What the worker hands to [`finish_organized`](super::finish_organized) and gets back from
//! [`read_source_details`](super::read_source_details). The worker has no lopdf, so values that
//! come out of a PDF's dictionaries stay behind private fields.

use lopdf::Object;

/// Where an outline item or a link leads in the output.
#[derive(Clone, Debug, PartialEq)]
pub enum LinkTarget {
    Uri(String),
    /// `page` is the position in the output, counted from 0.
    Page {
        page: u32,
        view: DestinationView,
    },
}

/// How a destination shows its page: the PDF destination types. A coordinate or zoom the
/// destination leaves out is `None` and is written as `null`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum DestinationView {
    Xyz {
        left: Option<f32>,
        top: Option<f32>,
        zoom: Option<f32>,
    },
    Fit,
    FitH {
        top: Option<f32>,
    },
    FitV {
        left: Option<f32>,
    },
    FitR {
        left: f32,
        bottom: f32,
        right: f32,
        top: f32,
    },
    FitB,
    FitBH {
        top: Option<f32>,
    },
    FitBV {
        left: Option<f32>,
    },
}

/// One bookmark of the output with the bookmarks below it. An item without a target is a heading.
#[derive(Clone, Debug, PartialEq)]
pub struct OutlineItem {
    pub title: String,
    pub target: Option<LinkTarget>,
    pub children: Vec<OutlineItem>,
}

/// A link of the output.
#[derive(Clone, Debug, PartialEq)]
pub struct OutputLink {
    /// The page of the output the link sits on, counted from 0.
    pub page: u32,
    /// Left, bottom, right and top in points, in the page's own coordinates.
    pub rect: [f32; 4],
    pub target: LinkTarget,
    pub style: LinkStyleSource,
}

/// Where the frame of a link comes from. pdfium never draws the frame, but other viewers do.
#[derive(Clone, Debug, PartialEq)]
pub enum LinkStyleSource {
    /// A page that was not flattened: pdfium copied its link annotations into the output. The
    /// frame is taken from the `nth` of them (counted from 0, links only) when its rectangle is
    /// the link's.
    Copied { nth: u32 },
    /// A page that was flattened, which drops its annotations: the frame read from the source
    /// PDF, or `None` if it could not be read (the link then has no frame).
    Source(Option<LinkStyle>),
}

/// The values of the link keys `/Border`, `/C`, `/H` and `/BS`.
#[derive(Clone, Debug, PartialEq)]
pub struct LinkStyle(pub(super) Vec<(&'static [u8], Object)>);

/// The values of the catalog keys `/PageLayout`, `/PageMode` and `/ViewerPreferences`.
#[derive(Clone, Debug, PartialEq)]
pub struct ViewSettings(pub(super) Vec<(&'static [u8], Object)>);

/// What is attached to the output again after pdfium has copied the pages.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Relinking {
    pub outline: Vec<OutlineItem>,
    pub links: Vec<OutputLink>,
    pub view: Option<ViewSettings>,
}

/// A link on a page of a source PDF whose frame is wanted.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SourceLinkKey {
    /// The page of the source, counted from 0.
    pub page: u32,
    /// The link among the page's link annotations, counted from 0.
    pub nth: u32,
    /// Left, bottom, right and top as pdfium reported them.
    pub rect: [f32; 4],
}

/// What [`read_source_details`](super::read_source_details) found in a source PDF.
#[derive(Clone, Debug, PartialEq)]
pub struct SourceDetails {
    pub view: Option<ViewSettings>,
    /// One entry for each requested link, in the same order; `None` where the frame could not be
    /// found.
    pub styles: Vec<Option<LinkStyle>>,
}
