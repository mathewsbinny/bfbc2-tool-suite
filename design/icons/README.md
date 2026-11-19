# The application icon

The suite's executables (`bfbc2.exe`, `bfbc2-editor.exe`, `mapexport.exe`), the map editor's window and the hub
(`BFBC2 Tool Suite.pyw`) show one icon: an M67 grenade, **the author's own render** (`source/m67_render.png`),
published with the suite under its license.

| File | What it is |
| --- | --- |
| `app.ico` | the icon: 256, 128, 64, 48, 32, 24 and 16 pixels, each PNG-compressed |
| `app_512.png` | a 512-pixel preview of it |
| `source/m67_render.png` | the author's render it is made from |
| `make_icon.py` | makes `app.ico` and `app_512.png` from the render (numpy, Pillow) |

Made again with `python design\icons\make_icon.py design\icons\source\m67_render.png design\icons` (byte for byte
the committed files; `--sheet` also writes `app_sizes.png`, every size on dark and light grounds, for a look).

The icon is looked for in this order:

1. `BFBC2_APP_ICON` - an environment variable with the path of an `.ico`;
2. `design/icons/local/app.ico` - a personal override. The folder is gitignored: nothing in it is ever committed;
3. `design/icons/app.ico` - the committed icon above;
4. none - everything still builds and runs, without an icon.

See README.md, "The application icon", for building with it.
