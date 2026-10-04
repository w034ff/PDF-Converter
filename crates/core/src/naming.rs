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

/// Allocates the next available filename using the `name (n).ext` scheme (design §6.4).
///
/// Collision checks against `used_lower` are case-insensitive.
/// When an available name is found, its lowercase form is inserted into `used_lower`,
/// and the candidate (preserving the original casing of `base_stem`) is returned.
fn allocate_unique_name(base_stem: &str, ext: &str, used_lower: &mut HashSet<String>) -> String {
    let ext_trimmed = ext.trim_start_matches('.');
    let mut n = 0usize;
    loop {
        let candidate = match (n, ext_trimmed.is_empty()) {
            (0, true) => base_stem.to_string(),
            (0, false) => format!("{base_stem}.{ext_trimmed}"),
            (_, true) => format!("{base_stem} ({n})"),
            (_, false) => format!("{base_stem} ({n}).{ext_trimmed}"),
        };

        let candidate_lower = candidate.to_lowercase();
        if !used_lower.contains(&candidate_lower) {
            used_lower.insert(candidate_lower);
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
        resolved[original_index] = allocate_unique_name(stem, "pdf", &mut used_lower);
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
        result[entry.input_idx][entry.page_idx] =
            allocate_unique_name(&base_stem, ext, &mut used_lower);
    }

    result
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

        // PDF to image rollover:
        let pdf_inputs = vec![PdfToImageInput {
            filename: "doc.pdf".to_string(),
            total_pages: 5,
            pages: vec![1],
        }];
        let mut existing_pdf = HashSet::new();
        existing_pdf.insert("doc_p1.png".to_string());
        existing_pdf.insert("doc_p1 (1).png".to_string());
        let pdf_result = resolve_pdf_to_image_names(&pdf_inputs, "png", &existing_pdf);
        assert_eq!(pdf_result, vec![vec!["doc_p1 (2).png"]]);
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
    fn pdf_to_image_case_differing_existing_file() {
        // Existing file differs only in case:
        let inputs = vec![PdfToImageInput {
            filename: "report.pdf".to_string(),
            total_pages: 5,
            pages: vec![1],
        }];
        let mut existing = HashSet::new();
        existing.insert("REPORT_P1.PNG".to_string());
        let result = resolve_pdf_to_image_names(&inputs, "png", &existing);
        assert_eq!(result, vec![vec!["report_p1 (1).png"]]);

        // Rollover with case differences:
        existing.insert("report_p1 (1).PNG".to_string());
        let result2 = resolve_pdf_to_image_names(&inputs, "png", &existing);
        assert_eq!(result2, vec![vec!["report_p1 (2).png"]]);
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
        assert_eq!(page_number_width(10), 2);
        assert_eq!(page_number_width(120), 3);

        let inputs = vec![
            PdfToImageInput {
                filename: "doc9.pdf".to_string(),
                total_pages: 9,
                pages: vec![1, 9],
            },
            PdfToImageInput {
                filename: "doc10.pdf".to_string(),
                total_pages: 10,
                pages: vec![1, 9, 10],
            },
            PdfToImageInput {
                filename: "doc120.pdf".to_string(),
                total_pages: 120,
                pages: vec![1, 10, 120],
            },
        ];
        let existing = HashSet::new();
        let result = resolve_pdf_to_image_names(&inputs, "png", &existing);
        assert_eq!(result[0], vec!["doc9_p1.png", "doc9_p9.png"]);
        assert_eq!(
            result[1],
            vec!["doc10_p01.png", "doc10_p09.png", "doc10_p10.png"]
        );
        assert_eq!(
            result[2],
            vec!["doc120_p001.png", "doc120_p010.png", "doc120_p120.png"]
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
    }

    #[test]
    fn pdf_to_image_order_invariance_with_collisions() {
        // Test PDF -> Image input order invariance where inputs collide with each other.
        // "DOC.pdf" vs "doc.pdf" both targeting page 1:
        // 'D' < 'd', so DOC.pdf gets "DOC_p1.png" and doc.pdf gets "doc_p1 (1).png"
        // regardless of which order they appear in the input list.
        let pdf_upper = PdfToImageInput {
            filename: "DOC.pdf".to_string(),
            total_pages: 5,
            pages: vec![1],
        };
        let pdf_lower = PdfToImageInput {
            filename: "doc.pdf".to_string(),
            total_pages: 5,
            pages: vec![1],
        };

        let res_1 = resolve_pdf_to_image_names(
            &[pdf_lower.clone(), pdf_upper.clone()],
            "png",
            &HashSet::new(),
        );
        assert_eq!(res_1[0], vec!["doc_p1 (1).png"]);
        assert_eq!(res_1[1], vec!["DOC_p1.png"]);

        let res_2 = resolve_pdf_to_image_names(&[pdf_upper, pdf_lower], "png", &HashSet::new());
        assert_eq!(res_2[0], vec!["DOC_p1.png"]);
        assert_eq!(res_2[1], vec!["doc_p1 (1).png"]);

        // Additional test with existing collision + multiple colliding inputs:
        let mut existing = HashSet::new();
        existing.insert("report_p1.png".to_string());

        let pdf_a = PdfToImageInput {
            filename: "REPORT.pdf".to_string(),
            total_pages: 5,
            pages: vec![1],
        };
        let pdf_b = PdfToImageInput {
            filename: "report.pdf".to_string(),
            total_pages: 5,
            pages: vec![1],
        };

        // 'R' (0x52) < 'r' (0x72).
        // REPORT.pdf is sorted first -> collides with existing "report_p1.png", gets "REPORT_p1 (1).png"
        // report.pdf is sorted second -> collides with both, gets "report_p1 (2).png"
        let order_ab =
            resolve_pdf_to_image_names(&[pdf_a.clone(), pdf_b.clone()], "png", &existing);
        assert_eq!(order_ab[0], vec!["REPORT_p1 (1).png"]);
        assert_eq!(order_ab[1], vec!["report_p1 (2).png"]);

        let order_ba = resolve_pdf_to_image_names(&[pdf_b, pdf_a], "png", &existing);
        assert_eq!(order_ba[0], vec!["report_p1 (2).png"]);
        assert_eq!(order_ba[1], vec!["REPORT_p1 (1).png"]);
    }
}
