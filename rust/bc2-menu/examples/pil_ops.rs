//! One `bc2_menu::pil` / `modeart` picture operation on a raw RGBA file, for `tests/pil_parity.py` (round OR5), which
//! holds the results against Pillow and numpy:
//!
//!     pil_ops resize <in.raw> <w> <h> <out.raw> <new w> <new h>
//!     pil_ops layer <in.raw> <w> <h> <out.raw> <base.raw> <x> <y>    base (same size as out) with in pasted through
//!                                                                    its alpha onto a clear layer, composited over
//!     pil_ops cutout|fit <in.raw> <w> <h> <out.raw>
//!     pil_ops decode <in.dxt5> <w> <h> <out.raw>
//!     pil_ops encode <in.raw> <w> <h> <out.dxt5>
//!     pil_ops untile <in.dxt> <w> <h> <out.dxt>

#![allow(clippy::many_single_char_names, reason = "the arguments' short names")]
#![allow(clippy::unwrap_used, clippy::indexing_slicing, clippy::print_stderr)]

use bc2_menu::modeart;
use bc2_menu::pil::{self, Rgba};

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let n = |i: usize| a[i].parse::<usize>().unwrap();
    let i = |k: usize| a[k].parse::<i64>().unwrap();
    let raw = std::fs::read(&a[1]).unwrap();
    let (w, h) = (n(2), n(3));
    let img = || Rgba { w, h, px: raw.clone() };
    let out: Vec<u8> = match a[0].as_str() {
        "resize" => img().resize_lanczos(n(5), n(6)).px,
        "layer" => {
            let base = std::fs::read(&a[5]).unwrap();
            let (bw, bh) = (n(8), n(9));
            let mut b = Rgba { w: bw, h: bh, px: base };
            let mut layer = Rgba::filled(bw, bh, [0; 4]);
            layer.paste_masked(&img(), i(6), i(7));
            b.alpha_composite(&layer);
            b.px
        }
        "cutout" => modeart::cutout(&img()).px,
        "fit" => modeart::fit(&img()).px,
        "decode" => pil::dxt5_decode(&raw, w, h).unwrap().px,
        "encode" => pil::dxt5_encode(&img()).concat(),
        "untile" => pil::xbox_untile_dxt(&raw, w, h).unwrap(),
        "crop" => img().crop(i(5), i(6), i(7), i(8)).px,
        "flip" => img().flip_lr().px,
        op => {
            eprintln!("unknown op {op}");
            std::process::exit(2);
        }
    };
    std::fs::write(&a[4], out).unwrap();
}
