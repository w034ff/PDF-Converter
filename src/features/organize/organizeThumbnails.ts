import { createThumbnailCache } from "../items/thumbnailCache";

/** About 55 screens of the grid at its usual size. */
const ORGANIZE_THUMBNAIL_CACHE_PAGES = 1000;

/**
 * The pictures of the grid's pages. The grid draws only the rows near the view
 * and the tab goes when another opens, so this outlives both. Nothing drops a
 * removed PDF's pictures: Rust never gives an ID twice, so they only wait
 * their turn to go.
 */
export const organizeThumbnails = createThumbnailCache(
  ORGANIZE_THUMBNAIL_CACHE_PAGES,
);
