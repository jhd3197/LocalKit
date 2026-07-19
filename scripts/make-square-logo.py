from PIL import Image

im = Image.open("assets/logo.png")
s = max(im.size)
canvas = Image.new("RGBA", (s, s), (0, 0, 0, 0))
canvas.paste(im, ((s - im.width) // 2, (s - im.height) // 2), im)
canvas.save("assets/logo-square.png")
print("ok", canvas.size)
