"""Draws the horizontal page (a) and the mixed page (c) for the selection spike.

A4 at 200 dpi (1654 x 2339 px), white with black text, IBM Plex Sans JP, the
same size and font as ocr-spike's vertical pages. Needs Pillow with raqm
(vertical text uses direction="ttb" with the font's vert feature).

    python gen_pages.py <IBMPlexSansJP-Regular.otf> <IBMPlexSansJP-Bold.otf> <out dir>
"""
import sys
from PIL import Image, ImageDraw, ImageFont

REG, BOLD, OUT = sys.argv[1], sys.argv[2], sys.argv[3]
W, H = 1654, 2339


def font(size, bold=False):
    return ImageFont.truetype(BOLD if bold else REG, size)


def wrap(text, f, width):
    lines, cur = [], ""
    for c in text:
        if f.getlength(cur + c) > width and cur:
            lines.append(cur)
            cur = c.lstrip()
        else:
            cur += c
    if cur:
        lines.append(cur)
    return lines


def para(d, text, f, x, y, width, lead):
    for line in wrap(text, f, width):
        d.text((x, y), line, font=f, fill=0)
        y += lead
    return y


def vcolumns(d, text, f, right, top, rows, pitch):
    """Columns of `rows` characters from `right` leftwards."""
    cols = [text[i : i + rows] for i in range(0, len(text), rows)]
    x = right
    for c in cols:
        d.text((x, top), c, font=f, fill=0, direction="ttb", features=["vert"], anchor="mt")
        x -= pitch
    return x


BODY_L = (
    "光学文字認識（OCR）は、紙の書類を写した画像から文字を読み取り、"
    "検索やコピーのできる文字に変える技術である。近年は深層学習によって精度が大きく上がり、"
    "日本語のように字の種類が多い言語でも、印刷された文書ならほぼ正しく読めるようになった。"
    "一方で、読み取った文字を画面の上で選ぶ体験は、元の文書の見た目と文字の位置が合っていないと損なわれる。"
)
BODY_R = (
    "そこで、ページの絵の上に透明な文字を重ね、利用者がドラッグで選べるようにする。"
    "2 段組みの文書では、左の段を上から下へ読み終えてから右の段へ移る。"
    "選択もこの読み順に従うべきで、段の間の余白にマウスが出ても選択が途切れてはならない。"
    "見出しや表の短い欄も同じ列の中で扱う。"
)

# (a) horizontal: heading, two columns, a short table.
img = Image.new("L", (W, H), 255)
d = ImageDraw.Draw(img)
d.text((140, 150), "文字の選択の試験", font=font(72, True), fill=0)
d.text((140, 260), "OCR の結果を画面で選ぶための試作 2026", font=font(36), fill=0)
f = font(36)
col_w = (W - 140 * 2 - 80) // 2
para(d, BODY_L, f, 140, 400, col_w, 60)
para(d, BODY_R, f, 140 + col_w + 80, 400, col_w, 60)
d.text((140, 1200), "表 1　読み取りの設定", font=font(40, True), fill=0)
rows = [("項目", "値", "備考"), ("モデル", "small", "既定"), ("解像度", "200 dpi", "A4"), ("言語", "日本語", "縦横")]
for i, (a, b, c) in enumerate(rows):
    y = 1290 + i * 80
    for x, t in ((140, a), (600, b), (1000, c)):
        d.text((x, y), t, font=font(36, i == 0), fill=0)
    d.line((140, y + 64, 1400, y + 64), fill=0, width=2)
para(d, "以上の設定で、ページの全体を読み取った。結果の座標はページの pt で持つ。", f, 140, 1700, W - 280, 60)
img.save(f"{OUT}/h1_two_columns.png")

# (c) mixed: horizontal heading and paragraph, a vertical block, a horizontal note.
img = Image.new("L", (W, H), 255)
d = ImageDraw.Draw(img)
d.text((140, 150), "縦と横の混ざったページ", font=font(64, True), fill=0)
para(d, "この段落は横書きである。右の囲みは縦書きで、列を右から左へ読む。", font(36), 140, 300, W - 280, 60)
VTEXT = "「吾輩は猫である。」名前はまだない。どこで生れたかとんと見当がつかぬ。何でも薄暗いじめじめした所でニャーニャー泣いていた事だけは記憶している。"
vcolumns(d, VTEXT, font(44), W - 200, 520, 18, 76)
para(d, "左の欄は横書きの説明である。縦書きの囲みを読み終えたら、ここへ戻る。", font(36), 140, 520, 640, 60)
para(d, "ページの下の注も横書きで、最後に読む。数字 123 と英字 ABC を含む。", font(32), 140, 1700, W - 280, 56)
img.save(f"{OUT}/m1_mixed.png")
print("ok")
