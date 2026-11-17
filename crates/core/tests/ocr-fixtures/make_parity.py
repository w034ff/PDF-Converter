"""Makes ocr_parity.pdf and ocr_parity.paddle.json (design ocr §11, 照合).

ocr_parity.pdf holds pages 2 and 3 of ocr_pages.pdf (vertical tiers, two
columns) as gray images, rendered by pdfium with the longer side 1600 px. Each
page is as many points as its image has pixels and the image fills it, so the
worker renders it back to the same pixels. The PNG data goes into the PDF
unchanged (FlateDecode with the PNG predictor).

ocr_parity.paddle.json is what PaddleOCR 3.7.0 reads on those images with
PP-OCRv6 small and its defaults (no document orientation, unwarping or line
orientation): the expected lines of the parity test in
crates/worker/tests/ocr.rs, in pixels, which are points on these pages.

The maintainer runs this when ocr_pages.pdf or the models change; the tests
never do. It needs the network once, for PaddleOCR's models:

    python -m venv venv
    venv/bin/pip install paddlepaddle==3.3.1 paddleocr==3.7.0 pypdfium2==5.15.2
    PADDLE_PDX_MODEL_SOURCE=huggingface venv/bin/python make_parity.py
    npx prettier --write ocr_parity.paddle.json
"""

import json
import struct
import tempfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
SOURCE = HERE / "ocr_pages.pdf"
PAGES = [2, 3]
LONG_SIDE_PX = 1600


def render(path: Path, page_number: int, out: Path) -> None:
    import pypdfium2 as pdfium

    page = pdfium.PdfDocument(str(path))[page_number - 1]
    scale = LONG_SIDE_PX / max(page.get_size())
    page.render(scale=scale, grayscale=True).to_pil().convert("L").save(out)


def png_data(path: Path) -> tuple[int, int, bytes]:
    data = path.read_bytes()
    assert data[:8] == b"\x89PNG\r\n\x1a\n"
    pos, idat, header = 8, b"", None
    while pos < len(data):
        length, kind = struct.unpack(">I4s", data[pos : pos + 8])
        body = data[pos + 8 : pos + 8 + length]
        if kind == b"IHDR":
            header = struct.unpack(">IIBBBBB", body)
        elif kind == b"IDAT":
            idat += body
        pos += 12 + length
    w, h, depth, colour, _, _, interlace = header
    assert depth == 8 and colour == 0 and interlace == 0, header
    return w, h, idat


def write_pdf(pngs: list[Path], out: Path) -> None:
    objects: list[bytes | None] = [None]
    kids = []

    def add(obj: bytes) -> int:
        objects.append(obj)
        return len(objects)

    for png in pngs:
        w, h, data = png_data(png)
        image = add(
            b"<< /Type /XObject /Subtype /Image /Width %d /Height %d "
            b"/ColorSpace /DeviceGray /BitsPerComponent 8 /Filter /FlateDecode "
            b"/DecodeParms << /Predictor 15 /Colors 1 /BitsPerComponent 8 /Columns %d >> "
            b"/Length %d >>\nstream\n" % (w, h, w, len(data))
            + data
            + b"\nendstream"
        )
        content = b"q %d 0 0 %d 0 0 cm /Im0 Do Q" % (w, h)
        contents = add(b"<< /Length %d >>\nstream\n" % len(content) + content + b"\nendstream")
        kids.append(
            add(
                b"<< /Type /Page /Parent 1 0 R /MediaBox [0 0 %d %d] "
                b"/Resources << /XObject << /Im0 %d 0 R >> >> /Contents %d 0 R >>"
                % (w, h, image, contents)
            )
        )
    objects[0] = b"<< /Type /Pages /Kids [%s] /Count %d >>" % (
        b" ".join(b"%d 0 R" % k for k in kids),
        len(kids),
    )
    catalog = add(b"<< /Type /Catalog /Pages 1 0 R >>")
    pdf = b"%PDF-1.7\n%\xe2\xe3\xcf\xd3\n"
    offsets = []
    for number, obj in enumerate(objects, 1):
        offsets.append(len(pdf))
        pdf += b"%d 0 obj\n" % number + obj + b"\nendobj\n"
    xref = len(pdf)
    pdf += b"xref\n0 %d\n0000000000 65535 f \n" % (len(objects) + 1)
    pdf += b"".join(b"%010d 00000 n \n" % o for o in offsets)
    pdf += b"trailer\n<< /Size %d /Root %d 0 R >>\nstartxref\n%d\n%%%%EOF\n" % (
        len(objects) + 1,
        catalog,
        xref,
    )
    out.write_bytes(pdf)


def read(pngs: list[Path]) -> list[dict]:
    import paddleocr
    from paddleocr import PaddleOCR

    ocr = PaddleOCR(
        text_detection_model_name="PP-OCRv6_small_det",
        text_recognition_model_name="PP-OCRv6_small_rec",
        use_doc_orientation_classify=False,
        use_doc_unwarping=False,
        use_textline_orientation=False,
        # oneDNN fails on these models with paddlepaddle 3.3.1.
        enable_mkldnn=False,
    )
    pages = []
    for png in pngs:
        result = ocr.predict(str(png))[0]
        regions = [
            {"text": text, "score": round(float(score), 6), "quad": quad.tolist()}
            for text, score, quad in zip(
                result["rec_texts"], result["rec_scores"], result["dt_polys"]
            )
        ]
        pages.append({"regions": regions})
    print(f"PaddleOCR {paddleocr.__version__}")
    return pages


def main() -> None:
    with tempfile.TemporaryDirectory() as work:
        pngs = []
        for number in PAGES:
            png = Path(work) / f"page-{number}.png"
            render(SOURCE, number, png)
            pngs.append(png)
        write_pdf(pngs, HERE / "ocr_parity.pdf")
        expected = {
            "generator": "PaddleOCR 3.7.0, PP-OCRv6_small_det and PP-OCRv6_small_rec, "
            "crates/core/tests/ocr-fixtures/make_parity.py",
            "pages": read(pngs),
        }
    (HERE / "ocr_parity.paddle.json").write_text(
        json.dumps(expected, ensure_ascii=False, indent=1) + "\n", encoding="utf-8"
    )


if __name__ == "__main__":
    main()
