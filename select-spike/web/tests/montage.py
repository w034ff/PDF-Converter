import sys
from PIL import Image, ImageDraw, ImageFont
S='/tmp/claude-0/shots'; Z='slint/out'
font = ImageFont.truetype('/home/user/ocrv/ocr-spike/data/fonts/IBMPlexSansJP-Regular.otf', 22)
def page_box(im, zoom):
    # find the white page: scan the row 200 px below the top for the first and last white pixel run
    w,h = im.size
    return im
for zoom, crop in ((1, (0, 50, 830, 1190)), (2, (0, 50, 1620, 1400))):
    for page, sc in (("h1_two_columns","cross-blocks"),("v2_two_tiers","cross-blocks"),("m1_mixed","cross-blocks"),("v4_dialogue","end-in-line-gap"),("h1_two_columns","end-in-line-gap")):
        srcs = [("ブラウザーの選択", f"{S}/chromium-native-{zoom}/{page}-{sc}.png"),
                ("HTML の自作", f"{S}/chromium-custom-{zoom}/{page}-{sc}.png"),
                ("Slint の自作", f"{Z}/z{zoom}/{page}-{sc}.png")]
        ims = [Image.open(p).convert("RGB").crop(crop) for _, p in srcs]
        sc_ = 0.5
        ims = [i.resize((int(i.width*sc_), int(i.height*sc_))) for i in ims]
        W = sum(i.width for i in ims) + 20*(len(ims)-1); H = ims[0].height + 40
        out = Image.new("RGB", (W, H), "white"); d = ImageDraw.Draw(out); x = 0
        for (label, _), i in zip(srcs, ims):
            d.text((x+4, 6), label, fill="black", font=font); out.paste(i, (x, 40)); x += i.width + 20
        out.save(f"screenshots/bands-{page}-{sc}-{zoom*100}.png", optimize=True)
        print(f"screenshots/bands-{page}-{sc}-{zoom*100}.png", out.size)
