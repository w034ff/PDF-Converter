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
