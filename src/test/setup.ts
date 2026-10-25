import "@testing-library/jest-dom/vitest";

// jsdom has no pointer capture; the drags of the crop tab need the calls to
// exist. A test that cares spies on them.
for (const name of ["setPointerCapture", "releasePointerCapture"]) {
  if (!(name in Element.prototype)) {
    Object.defineProperty(Element.prototype, name, {
      value: () => {},
      configurable: true,
      writable: true,
    });
  }
}

// jsdom has no scrolling either. A test that cares spies on scrollIntoView.
if (!("scrollIntoView" in Element.prototype)) {
  Object.defineProperty(Element.prototype, "scrollIntoView", {
    value: () => {},
    configurable: true,
    writable: true,
  });
}

// The page is dragged to scroll it. This one moves the offsets, so a test
// can read where a drag scrolled to.
if (!("scrollTo" in Element.prototype)) {
  Object.defineProperty(Element.prototype, "scrollTo", {
    value(this: Element, options: ScrollToOptions) {
      this.scrollLeft = options.left ?? this.scrollLeft;
      this.scrollTop = options.top ?? this.scrollTop;
    },
    configurable: true,
    writable: true,
  });
}
