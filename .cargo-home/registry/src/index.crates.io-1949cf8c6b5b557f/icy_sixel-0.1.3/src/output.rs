use std::io::Write;

use crate::{EncodePolicy, PaletteType};

#[derive(Default, PartialEq)]
pub struct sixel_node {
    pub pal: i32,
    pub sx: i32,
    pub mx: i32,
    pub map: Vec<u8>,
}

pub struct sixel_output<W: Write> {
    /* compatiblity flags */

    /* 0: 7bit terminal,
     * 1: 8bit terminal */
    pub(crate) has_8bit_control: bool,

    /* 0: the terminal has sixel scrolling
     * 1: the terminal does not have sixel scrolling */
    pub(crate) has_sixel_scrolling: bool,

    /* 1: the argument of repeat introducer(DECGRI) is not limitted
    0: the argument of repeat introducer(DECGRI) is limitted 255 */
    pub(crate) has_gri_arg_limit: bool,

    /* 0: DECSDM set (CSI ? 80 h) enables sixel scrolling
    1: DECSDM set (CSI ? 80 h) disables sixel scrolling */
    pub(crate) has_sdm_glitch: bool,

    /* 0: do not skip DCS envelope
     * 1: skip DCS envelope */
    pub(crate) skip_dcs_envelope: bool,

    /* PALETTETYPE_AUTO: select palette type automatically
     * PALETTETYPE_HLS : HLS color space
     * PALETTETYPE_RGB : RGB color space */
    pub palette_type: PaletteType,

    pub fn_write: W,

    pub save_pixel: u8,
    pub save_count: i32,
    pub active_palette: i32,

    pub nodes: Vec<sixel_node>,

    pub penetrate_multiplexer: bool,
    pub encode_policy: EncodePolicy,

    pub buffer: String,
}

impl<W: Write> sixel_output<W> {
    /// create new output context object
    pub fn new(fn_write: W) -> Self {
        Self {
            has_8bit_control: false,
            has_sdm_glitch: false,
            has_gri_arg_limit: true,
            skip_dcs_envelope: false,
            palette_type: PaletteType::Auto,
            fn_write,
            save_pixel: 0,
            save_count: 0,
            active_palette: -1,
            nodes: Vec::new(),
            penetrate_multiplexer: false,
            encode_policy: EncodePolicy::AUTO,
            has_sixel_scrolling: false,
            buffer: String::new(),
        }
    }

    /// get 8bit output mode which indicates whether it uses C1 control characters
    pub fn get_8bit_availability(&self) -> bool {
        self.has_8bit_control
    }

    /// set 8bit output mode state
    pub fn set_8bit_availability(&mut self, availability: bool) {
        self.has_8bit_control = availability;
    }

    /// set whether limit arguments of DECGRI('!') to 255
    ///   /* 0: don't limit arguments of DECGRI
    /// 1: limit arguments of DECGRI to 255 */
    pub fn set_gri_arg_limit(&mut self, value: bool) {
        self.has_gri_arg_limit = value;
    }

    /// set GNU Screen penetration feature enable or disable
    pub fn set_penetrate_multiplexer(&mut self, penetrate: bool) {
        self.penetrate_multiplexer = penetrate;
    }

    /// set whether we skip DCS envelope
    pub fn set_skip_dcs_envelope(&mut self, skip: bool) {
        self.skip_dcs_envelope = skip;
    }

    /// set palette type: RGB or HLS
    pub fn set_palette_type(&mut self, palettetype: PaletteType) {
        self.palette_type = palettetype;
    }

    /// set encodeing policy: auto, fast or size
    pub fn set_encode_policy(&mut self, encode_policy: EncodePolicy) {
        self.encode_policy = encode_policy;
    }
}

/* emacs Local Variables:      */
/* emacs mode: c               */
/* emacs tab-width: 4          */
/* emacs indent-tabs-mode: nil */
/* emacs c-basic-offset: 4     */
/* emacs End:                  */
/* vim: set expandtab ts=4 sts=4 sw=4 : */
/* EOF */

/*
 * Copyright (c) 2014-2019 Hayaki Saito
 *
 * Permission is hereby granted, free of charge, to any person obtaining a copy of
 * this software and associated documentation files (the "Software"), to deal in
 * the Software without restriction, including without limitation the rights to
 * use, copy, modify, merge, publish, distribute, sublicense, and/or sell copies of
 * the Software, and to permit persons to whom the Software is furnished to do so,
 * subject to the following conditions:
 *
 * The above copyright notice and this permission notice shall be included in all
 * copies or substantial portions of the Software.
 *
 * THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
 * IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY, FITNESS
 * FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR
 * COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER
 * IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN
 * CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.
 */
