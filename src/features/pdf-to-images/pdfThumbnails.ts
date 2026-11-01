import { createThumbnailCache } from "../items/thumbnailCache";

/** About 55 screens of the grid at its usual size. */
const PDF_THUMBNAIL_CACHE_PAGES = 1000;

/**
 * The pictures of the page grid, which draws only the rows near the view and
 * so would ask again for each page scrolled back to. Nothing drops a removed
 * PDF's pictures: Rust never gives an ID twice, so they only wait their turn
 * to go.
 */
export const pdfThumbnails = createThumbnailCache(PDF_THUMBNAIL_CACHE_PAGES);
