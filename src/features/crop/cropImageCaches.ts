import { createThumbnailCache } from "../items/thumbnailCache";

/** A page's picture is far larger than a thumbnail, so only the last few stay. */
const CROP_PAGE_IMAGE_CACHE_SIZE = 5;

const CROP_THUMBNAIL_CACHE_PAGES = 1000;

/*
 * The screen goes when another tab opens, so these outlive it. Nothing drops
 * the pictures of a closed PDF: Rust never gives an ID twice, so they only
 * wait their turn to go.
 */

/** The pictures of the pages, one for each size asked for (`usePageImage`). */
export const cropPageImages = createThumbnailCache(CROP_PAGE_IMAGE_CACHE_SIZE);

/** The pictures of the list of pages. */
export const cropThumbnails = createThumbnailCache(CROP_THUMBNAIL_CACHE_PAGES);
