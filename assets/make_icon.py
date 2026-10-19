# Regenerates ferrite.ico: python assets/make_icon.py
from pathlib import Path
from PIL import Image, ImageDraw, ImageFilter

S = 1024
lin = Image.linear_gradient("L").resize((S, S))
grad = Image.blend(lin, lin.rotate(90), 0.5)  # top-left light -> bottom-right dark
bg = Image.composite(Image.new("RGB", (S, S), "#c2310c"), Image.new("RGB", (S, S), "#ffa040"), grad)
mask = Image.new("L", (S, S))
ImageDraw.Draw(mask).rounded_rectangle((40, 40, S - 40, S - 40), 230, fill=255)
img = Image.new("RGBA", (S, S))
img.paste(bg, mask=mask)

bolt = [(590, 130), (250, 580), (480, 580), (400, 900), (780, 420), (545, 420), (650, 130)]
shadow = Image.new("L", (S, S))
ImageDraw.Draw(shadow).polygon([(x + 18, y + 24) for x, y in bolt], fill=110)
img.paste((90, 20, 0, 255), mask=shadow.filter(ImageFilter.GaussianBlur(22)))
ImageDraw.Draw(img).polygon(bolt, fill="white")
img.putalpha(Image.composite(img.getchannel("A"), Image.new("L", (S, S)), mask))

out = Path(__file__).with_name("ferrite.ico")
img.save(out, sizes=[(s, s) for s in (16, 24, 32, 48, 64, 128, 256)])
img.resize((256, 256), Image.LANCZOS).save(out.with_suffix(".png"))
