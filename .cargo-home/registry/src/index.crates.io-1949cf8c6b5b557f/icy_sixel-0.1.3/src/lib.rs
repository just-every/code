#![allow(
    clippy::identity_op,
    non_snake_case,
    non_camel_case_types,
    non_upper_case_globals,
    clippy::upper_case_acronyms,
    dead_code
)]

use std::error::Error;

use dither::sixel_dither;
use output::sixel_output;

pub mod dither;
pub mod output;
pub mod pixelformat;
pub mod quant;
pub mod tosixel;

/* limitations */
const SIXEL_OUTPUT_PACKET_SIZE: usize = 16384;
const SIXEL_PALETTE_MIN: usize = 2;
const SIXEL_PALETTE_MAX: usize = 256;
const SIXEL_USE_DEPRECATED_SYMBOLS: usize = 1;
const SIXEL_ALLOCATE_BYTES_MAX: usize = 10248 * 1024 * 128; /* up to 128M */
const SIXEL_WIDTH_LIMIT: usize = 1000000;
const SIXEL_HEIGHT_LIMIT: usize = 1000000;

/* loader settings */
const SIXEL_DEFALUT_GIF_DELAY: usize = 1;

/* return value */
pub type SixelResult<T> = Result<T, Box<dyn Error>>;

#[derive(Debug, Clone)]
pub enum SixelError {
    RuntimeError,       /* runtime error */
    LogicError,         /* logic error */
    FeatureError,       /* feature not enabled */
    LibcError,          /* errors caused by curl */
    CurlError,          /* errors occures in libc functions */
    JpegError,          /* errors occures in libjpeg functions */
    PngError,           /* errors occures in libpng functions */
    GdkError,           /* errors occures in gdk functions */
    GdError,            /* errors occures in gd functions */
    StbiError,          /* errors occures in stb_image functions */
    StbiwError,         /* errors occures in stb_image_write functions */
    INTERRUPTED,        /* interrupted by a signal */
    BadAllocation,      /* malloc() failed */
    BadArgument,        /* bad argument detected */
    BadInput,           /* bad input detected */
    BadIntegerOverflow, /* integer overflow */
    NotImplemented,     /* feature not implemented */
}

impl std::fmt::Display for SixelError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SixelError::RuntimeError => write!(f, "runtime error"),
            SixelError::LogicError => write!(f, "logic error"),
            SixelError::FeatureError => write!(f, "feature not enabled"),
            SixelError::LibcError => write!(f, "errors occures in libc functions"),
            SixelError::CurlError => write!(f, "errors caused by curl"),
            SixelError::JpegError => write!(f, "errors occures in libjpeg functions"),
            SixelError::PngError => write!(f, "errors occures in libpng functions"),
            SixelError::GdkError => write!(f, "errors occures in gdk functions"),
            SixelError::GdError => write!(f, "errors occures in gd functions"),
            SixelError::StbiError => write!(f, "errors occures in stb_image functions"),
            SixelError::StbiwError => write!(f, "errors occures in stb_image_write functions"),
            SixelError::INTERRUPTED => write!(f, "interrupted by a signal"),
            SixelError::BadAllocation => write!(f, "malloc() failed"),
            SixelError::BadArgument => write!(f, "bad argument detected"),
            SixelError::BadInput => write!(f, "bad input detected"),
            SixelError::BadIntegerOverflow => write!(f, "integer overflow"),
            SixelError::NotImplemented => write!(f, "feature not implemented"),
        }
    }
}
impl Error for SixelError {
    fn description(&self) -> &str {
        "use std::display"
    }

    fn source(&self) -> Option<&(dyn Error + 'static)> {
        None
    }

    fn cause(&self) -> Option<&dyn Error> {
        self.source()
    }
}

/*
typedef int SIXELSTATUS;
const SIXEL_OK                0x0000                          /* succeeded */
const SIXEL_FALSE             0x1000                          /* failed */


const SIXEL_SUCCEEDED(status) (((status) & 0x1000) == 0)
const SIXEL_FAILED(status)    (((status) & 0x1000) != 0)
*/

/// method for finding the largest dimension for splitting,
/// and sorting by that component
#[derive(Clone, Copy)]
pub enum MethodForLargest {
    /// choose automatically the method for finding the largest dimension
    Auto,
    /// simply comparing the range in RGB space
    Norm,
    /// transforming into luminosities before the comparison
    Lum,
}

/// method for choosing a color from the box
#[derive(Clone, Copy)]
pub enum MethodForRep {
    /// choose automatically the method for selecting
    /// representative color from each box
    Auto,
    /// choose the center of the box
    CenterBox,
    /// choose the average all the color in the box (specified in Heckbert's paper)
    AverageColors,
    /// choose the average all the pixels in the box
    Pixels,
}

#[derive(Clone, Copy)]
pub enum DiffusionMethod {
    /// choose diffusion type automatically
    Auto = 0,
    /// don't diffuse
    None = 1,
    /// diffuse with Bill Atkinson's method
    Atkinson = 2,
    /// diffuse with Floyd-Steinberg method
    FS = 3,
    /// diffuse with Jarvis, Judice & Ninke method
    JaJuNi = 4,
    /// diffuse with Stucki's method
    Stucki = 5,
    /// diffuse with Burkes' method
    Burkes = 6,
    /// positionally stable arithmetic dither
    ADither = 7,
    /// positionally stable arithmetic xor based dither
    XDither = 8,
}

/// quality modes
#[derive(Clone, Copy)]
pub enum Quality {
    /// choose quality mode automatically
    AUTO,
    /// high quality palette construction
    HIGH,
    /// low quality palette construction
    LOW,
    /// full quality palette construction
    FULL,
    /// high color
    HIGHCOLOR,
}

/* built-in dither */
#[derive(Clone, Copy)]
pub enum BuiltinDither {
    /// monochrome terminal with dark background
    MonoDark,
    /// monochrome terminal with light background
    MonoLight,
    /// x
    /// term 16color
    XTerm16,
    /// xterm 256color
    XTerm256,
    /// vt340 monochrome
    VT340Mono,
    /// vt340 color
    VT340Color,
    /// 1bit grayscale
    G1,
    /// 2bit grayscale
    G2,
    /// 4bit grayscale
    G4,
    /// 8bit grayscale
    G8,
}

/// offset value of pixelFormat
pub enum FormatType {
    COLOR,     // 0
    GRAYSCALE, // (1 << 6)
    PALETTE,   //    (1 << 7)
}

/// pixelformat type of input image
/// NOTE: for compatibility, the value of PIXELFORAMT_COLOR_RGB888 must be 3
#[derive(Clone, Copy, PartialEq)]
pub enum PixelFormat {
    RGB555 = 1,             //   (SIXEL_FORMATTYPE_COLOR     | 0x01) /* 15bpp */
    RGB565 = 2,             //   (SIXEL_FORMATTYPE_COLOR     | 0x02) /* 16bpp */
    RGB888 = 3,             //   (SIXEL_FORMATTYPE_COLOR     | 0x03) /* 24bpp */
    BGR555 = 4,             //   (SIXEL_FORMATTYPE_COLOR     | 0x04) /* 15bpp */
    BGR565 = 5,             //   (SIXEL_FORMATTYPE_COLOR     | 0x05) /* 16bpp */
    BGR888 = 6,             //   (SIXEL_FORMATTYPE_COLOR     | 0x06) /* 24bpp */
    ARGB8888 = 0x10,        // (SIXEL_FORMATTYPE_COLOR     | 0x10) /* 32bpp */
    RGBA8888 = 0x11,        // (SIXEL_FORMATTYPE_COLOR     | 0x11) /* 32bpp */
    ABGR8888 = 0x12,        // (SIXEL_FORMATTYPE_COLOR     | 0x12) /* 32bpp */
    BGRA8888 = 0x13,        // (SIXEL_FORMATTYPE_COLOR     | 0x13) /* 32bpp */
    G1 = (1 << 6),          //       (SIXEL_FORMATTYPE_GRAYSCALE | 0x00) /* 1bpp grayscale */
    G2 = (1 << 6) | 0x01,   //       (SIXEL_FORMATTYPE_GRAYSCALE | 0x01) /* 2bpp grayscale */
    G4 = (1 << 6) | 0x02,   //       (SIXEL_FORMATTYPE_GRAYSCALE | 0x02) /* 4bpp grayscale */
    G8 = (1 << 6) | 0x03,   //       (SIXEL_FORMATTYPE_GRAYSCALE | 0x03) /* 8bpp grayscale */
    AG88 = (1 << 6) | 0x13, //     (SIXEL_FORMATTYPE_GRAYSCALE | 0x13) /* 16bpp gray+alpha */
    GA88 = (1 << 6) | 0x23, //     (SIXEL_FORMATTYPE_GRAYSCALE | 0x23) /* 16bpp gray+alpha */
    PAL1 = (1 << 7),        //     (SIXEL_FORMATTYPE_PALETTE   | 0x00) /* 1bpp palette */
    PAL2 = (1 << 7) | 0x01, //     (SIXEL_FORMATTYPE_PALETTE   | 0x01) /* 2bpp palette */
    PAL4 = (1 << 7) | 0x02, //     (SIXEL_FORMATTYPE_PALETTE   | 0x02) /* 4bpp palette */
    PAL8 = (1 << 7) | 0x03, //     (SIXEL_FORMATTYPE_PALETTE   | 0x03) /* 8bpp palette */
}

pub enum PaletteType {
    /// choose palette type automatically
    Auto,
    /// HLS colorspace
    HLS,
    /// RGB colorspace
    RGB,
}

/// policies of SIXEL encoding
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum EncodePolicy {
    /// choose encoding policy automatically
    AUTO = 0,
    /// encode as fast as possible
    FAST = 1,
    /// encode to as small sixel sequence as possible
    SIZE = 2,
}

pub enum ResampleMethod {
    /// Use nearest neighbor method
    NEAREST,
    /// Use guaussian filter
    GAUSSIAN,
    /// Use hanning filter
    HANNING,
    /// Use hamming filter
    HAMMING,
    /// Use bilinear filter
    BILINEAR,
    /// Use welsh filter
    WELSH,
    /// Use bicubic filter
    BICUBIC,
    /// Use lanczos-2 filter
    LANCZOS2,
    /// Use lanczos-3 filter
    LANCZOS3,
    /// Use lanczos-4 filter
    LANCZOS4,
}
/* image format */
enum Format {
    GIF,   //         0x0 /* read only */
    PNG,   //         0x1 /* read/write */
    BMP,   //         0x2 /* read only */
    JPG,   //         0x3 /* read only */
    TGA,   //         0x4 /* read only */
    WBMP,  //         0x5 /* read only with --with-gd configure option */
    TIFF,  //         0x6 /* read only */
    SIXEL, //         0x7 /* read only */
    PNM,   //         0x8 /* read only */
    GD2,   //         0x9 /* read only with --with-gd configure option */
    PSD,   //         0xa /* read only */
    HDR,   //         0xb /* read only */
}

/* loop mode */
enum Loop {
    /// honer the setting of GIF header
    AUTO,
    /// always enable loop
    FORCE,
    /// always disable loop
    DISABLE,
}
/*
/* setopt flags */
const SIXEL_OPTFLAG_INPUT             ('i')  /* -i, --input: specify input file name. */
const SIXEL_OPTFLAG_OUTPUT            ('o')  /* -o, --output: specify output file name. */
const SIXEL_OPTFLAG_OUTFILE           ('o')  /* -o, --outfile: specify output file name. */
const SIXEL_OPTFLAG_7BIT_MODE         ('7')  /* -7, --7bit-mode: for 7bit terminals or printers (default) */
const SIXEL_OPTFLAG_8BIT_MODE         ('8')  /* -8, --8bit-mode: for 8bit terminals or printers */
const SIXEL_OPTFLAG_HAS_GRI_ARG_LIMIT ('R')  /* -R, --gri-limit: limit arguments of DECGRI('!') to 255 */
const SIXEL_OPTFLAG_COLORS            ('p')  /* -p COLORS, --colors=COLORS: specify number of colors */
const SIXEL_OPTFLAG_MAPFILE           ('m')  /* -m FILE, --mapfile=FILE: specify set of colors */
const SIXEL_OPTFLAG_MONOCHROME        ('e')  /* -e, --monochrome: output monochrome sixel image */
const SIXEL_OPTFLAG_INSECURE          ('k')  /* -k, --insecure: allow to connect to SSL sites without certs */
const SIXEL_OPTFLAG_INVERT            ('i')  /* -i, --invert: assume the terminal background color */
const SIXEL_OPTFLAG_HIGH_COLOR        ('I')  /* -I, --high-color: output 15bpp sixel image */
const SIXEL_OPTFLAG_USE_MACRO         ('u')  /* -u, --use-macro: use DECDMAC and DEVINVM sequences */
const SIXEL_OPTFLAG_MACRO_NUMBER      ('n')  /* -n MACRONO, --macro-number=MACRONO:
                                                  specify macro register number */
const SIXEL_OPTFLAG_COMPLEXION_SCORE  ('C')  /* -C COMPLEXIONSCORE, --complexion-score=COMPLEXIONSCORE:
                                                  specify an number argument for the score of
                                                  complexion correction. */
const SIXEL_OPTFLAG_IGNORE_DELAY      ('g')  /* -g, --ignore-delay: render GIF animation without delay */
const SIXEL_OPTFLAG_STATIC            ('S')  /* -S, --static: render animated GIF as a static image */
const SIXEL_OPTFLAG_DIFFUSION         ('d')  /* -d DIFFUSIONTYPE, --diffusion=DIFFUSIONTYPE:
                                                  choose diffusion method which used with -p option.
                                                  DIFFUSIONTYPE is one of them:
                                                    auto     -> choose diffusion type
                                                                automatically (default)
                                                    none     -> do not diffuse
                                                    fs       -> Floyd-Steinberg method
                                                    atkinson -> Bill Atkinson's method
                                                    jajuni   -> Jarvis, Judice & Ninke
                                                    stucki   -> Stucki's method
                                                    burkes   -> Burkes' method
                                                    a_dither -> positionally stable
                                                                arithmetic dither
                                                    a_dither -> positionally stable
                                                                arithmetic xor based dither
                                                */
const SIXEL_OPTFLAG_FIND_LARGEST      ('f')  /* -f FINDTYPE, --find-largest=FINDTYPE:
                                                  choose method for finding the largest
                                                  dimension of median cut boxes for
                                                  splitting, make sense only when -p
                                                  option (color reduction) is
                                                  specified
                                                  FINDTYPE is one of them:
                                                    auto -> choose finding method
                                                            automatically (default)
                                                    norm -> simply comparing the
                                                            range in RGB space
                                                    lum  -> transforming into
                                                            luminosities before the
                                                            comparison
                                                */
const SIXEL_OPTFLAG_SELECT_COLOR      ('s')  /* -s SELECTTYPE, --select-color=SELECTTYPE
                                                  choose the method for selecting
                                                  representative color from each
                                                  median-cut box, make sense only
                                                  when -p option (color reduction) is
                                                  specified
                                                  SELECTTYPE is one of them:
                                                    auto      -> choose selecting
                                                                 method automatically
                                                                 (default)
                                                    center    -> choose the center of
                                                                 the box
                                                    average    -> calculate the color
                                                                 average into the box
                                                    histogram -> similar with average
                                                                 but considers color
                                                                 histogram
                                                */
const SIXEL_OPTFLAG_CROP              ('c')  /* -c REGION, --crop=REGION:
                                                  crop source image to fit the
                                                  specified geometry. REGION should
                                                  be formatted as '%dx%d+%d+%d'
                                                */
const SIXEL_OPTFLAG_WIDTH             ('w')  /* -w WIDTH, --width=WIDTH:
                                                  resize image to specified width
                                                  WIDTH is represented by the
                                                  following syntax
                                                    auto       -> preserving aspect
                                                                  ratio (default)
                                                    <number>%  -> scale width with
                                                                  given percentage
                                                    <number>   -> scale width with
                                                                  pixel counts
                                                    <number>px -> scale width with
                                                                  pixel counts
                                                */
const SIXEL_OPTFLAG_HEIGHT            ('h')  /* -h HEIGHT, --height=HEIGHT:
                                                   resize image to specified height
                                                   HEIGHT is represented by the
                                                   following syntax
                                                     auto       -> preserving aspect
                                                                   ratio (default)
                                                     <number>%  -> scale height with
                                                                   given percentage
                                                     <number>   -> scale height with
                                                                   pixel counts
                                                     <number>px -> scale height with
                                                                   pixel counts
                                                */
const SIXEL_OPTFLAG_RESAMPLING        ('r')  /* -r RESAMPLINGTYPE, --resampling=RESAMPLINGTYPE:
                                                  choose resampling filter used
                                                  with -w or -h option (scaling)
                                                  RESAMPLINGTYPE is one of them:
                                                    nearest  -> Nearest-Neighbor
                                                                method
                                                    gaussian -> Gaussian filter
                                                    hanning  -> Hanning filter
                                                    hamming  -> Hamming filter
                                                    bilinear -> Bilinear filter
                                                                (default)
                                                    welsh    -> Welsh filter
                                                    bicubic  -> Bicubic filter
                                                    lanczos2 -> Lanczos-2 filter
                                                    lanczos3 -> Lanczos-3 filter
                                                    lanczos4 -> Lanczos-4 filter
                                                */
const SIXEL_OPTFLAG_QUALITY           ('q')  /* -q QUALITYMODE, --quality=QUALITYMODE:
                                                  select quality of color
                                                  quanlization.
                                                    auto -> decide quality mode
                                                            automatically (default)
                                                    low  -> low quality and high
                                                            speed mode
                                                    high -> high quality and low
                                                            speed mode
                                                    full -> full quality and careful
                                                            speed mode
                                                */
const SIXEL_OPTFLAG_LOOPMODE          ('l')  /* -l LOOPMODE, --loop-control=LOOPMODE:
                                                  select loop control mode for GIF
                                                  animation.
                                                    auto    -> honor the setting of
                                                               GIF header (default)
                                                    force   -> always enable loop
                                                    disable -> always disable loop
                                                */
const SIXEL_OPTFLAG_PALETTE_TYPE      ('t')  /* -t PALETTETYPE, --palette-type=PALETTETYPE:
                                                  select palette color space type
                                                    auto -> choose palette type
                                                            automatically (default)
                                                    hls  -> use HLS color space
                                                    rgb  -> use RGB color space
                                                */
const SIXEL_OPTFLAG_BUILTIN_PALETTE   ('b')  /* -b BUILTINPALETTE, --builtin-palette=BUILTINPALETTE:
                                                  select built-in palette type
                                                    xterm16    -> X default 16 color map
                                                    xterm256   -> X default 256 color map
                                                    vt340mono  -> VT340 monochrome map
                                                    vt340color -> VT340 color map
                                                    gray1      -> 1bit grayscale map
                                                    gray2      -> 2bit grayscale map
                                                    gray4      -> 4bit grayscale map
                                                    gray8      -> 8bit grayscale map
                                                */
const SIXEL_OPTFLAG_ENCODE_POLICY     ('E')  /* -E ENCODEPOLICY, --encode-policy=ENCODEPOLICY:
                                                  select encoding policy
                                                    auto -> choose encoding policy
                                                            automatically (default)
                                                    fast -> encode as fast as possible
                                                    size -> encode to as small sixel
                                                            sequence as possible
                                                */
const SIXEL_OPTFLAG_BGCOLOR           ('B')  /* -B BGCOLOR, --bgcolor=BGCOLOR:
                                                  specify background color
                                                  BGCOLOR is represented by the
                                                  following syntax
                                                    #rgb
                                                    #rrggbb
                                                    #rrrgggbbb
                                                    #rrrrggggbbbb
                                                    rgb:r/g/b
                                                    rgb:rr/gg/bb
                                                    rgb:rrr/ggg/bbb
                                                    rgb:rrrr/gggg/bbbb
                                                */
const SIXEL_OPTFLAG_PENETRATE         ('P')  /* -P, --penetrate:
                                                  penetrate GNU Screen using DCS
                                                  pass-through sequence */
const SIXEL_OPTFLAG_PIPE_MODE         ('D')  /* -D, --pipe-mode: (deprecated)
                                                  read source images from stdin continuously */
const SIXEL_OPTFLAG_VERBOSE           ('v')  /* -v, --verbose: show debugging info */
const SIXEL_OPTFLAG_VERSION           ('V')  /* -V, --version: show version and license info */
const SIXEL_OPTFLAG_HELP              ('H')  /* -H, --help: show this help */

#if SIXEL_USE_DEPRECATED_SYMBOLS
/* output character size */
enum characterSize {
    CSIZE_7BIT = 0,  /* 7bit character */
    CSIZE_8BIT = 1   /* 8bit character */
};

/* method for finding the largest dimension for splitting,
 * and sorting by that component */
enum methodForLargest {
    LARGE_AUTO = 0,  /* choose automatically the method for finding the largest
                        dimension */
    LARGE_NORM = 1,  /* simply comparing the range in RGB space */
    LARGE_LUM  = 2   /* transforming into luminosities before the comparison */
};

/* method for choosing a color from the box */
enum methodForRep {
    REP_AUTO           = 0, /* choose automatically the method for selecting
                               representative color from each box */
    REP_CENTER_BOX     = 1, /* choose the center of the box */
    REP_AVERAGE_COLORS = 2, /* choose the average all the color
                               in the box (specified in Heckbert's paper) */
    REP_AVERAGE_PIXELS = 3  /* choose the average all the pixels in the box */
};

/* method for diffusing */
enum methodForDiffuse {
    DIFFUSE_AUTO     = 0, /* choose diffusion type automatically */
    DIFFUSE_NONE     = 1, /* don't diffuse */
    DIFFUSE_ATKINSON = 2, /* diffuse with Bill Atkinson's method */
    DIFFUSE_FS       = 3, /* diffuse with Floyd-Steinberg method */
    DIFFUSE_JAJUNI   = 4, /* diffuse with Jarvis, Judice & Ninke method */
    DIFFUSE_STUCKI   = 5, /* diffuse with Stucki's method */
    DIFFUSE_BURKES   = 6, /* diffuse with Burkes' method */
    DIFFUSE_A_DITHER = 7, /* positionally stable arithmetic dither */
    DIFFUSE_X_DITHER = 8  /* positionally stable arithmetic xor based dither */
};

/* quality modes */
enum qualityMode {
    QUALITY_AUTO      = 0, /* choose quality mode automatically */
    QUALITY_HIGH      = 1, /* high quality palette construction */
    QUALITY_LOW       = 2, /* low quality palette construction */
    QUALITY_FULL      = 3, /* full quality palette construction */
    QUALITY_HIGHCOLOR = 4  /* high color */
};

/* built-in dither */
enum builtinDither {
    BUILTIN_MONO_DARK   = 0, /* monochrome terminal with dark background */
    BUILTIN_MONO_LIGHT  = 1, /* monochrome terminal with dark background */
    BUILTIN_XTERM16     = 2, /* xterm 16color */
    BUILTIN_XTERM256    = 3, /* xterm 256color */
    BUILTIN_VT340_MONO  = 4, /* vt340 monochrome */
    BUILTIN_VT340_COLOR = 5  /* vt340 color */
};

/* offset value of enum pixelFormat */
enum formatType {
    FORMATTYPE_COLOR     = 0,
    FORMATTYPE_GRAYSCALE = 1 << 6,
    FORMATTYPE_PALETTE   = 1 << 7
};

/* pixelformat type of input image
   NOTE: for compatibility, the value of PIXELFORAMT_COLOR_RGB888 must be 3 */
enum pixelFormat {
    PIXELFORMAT_RGB555   = FORMATTYPE_COLOR     | 0x01, /* 15bpp */
    PIXELFORMAT_RGB565   = FORMATTYPE_COLOR     | 0x02, /* 16bpp */
    PIXELFORMAT_RGB888   = FORMATTYPE_COLOR     | 0x03, /* 24bpp */
    PIXELFORMAT_BGR555   = FORMATTYPE_COLOR     | 0x04, /* 15bpp */
    PIXELFORMAT_BGR565   = FORMATTYPE_COLOR     | 0x05, /* 16bpp */
    PIXELFORMAT_BGR888   = FORMATTYPE_COLOR     | 0x06, /* 24bpp */
    PIXELFORMAT_ARGB8888 = FORMATTYPE_COLOR     | 0x10, /* 32bpp */
    PIXELFORMAT_RGBA8888 = FORMATTYPE_COLOR     | 0x11, /* 32bpp */
    PIXELFORMAT_G1       = FORMATTYPE_GRAYSCALE | 0x00, /* 1bpp grayscale */
    PIXELFORMAT_G2       = FORMATTYPE_GRAYSCALE | 0x01, /* 2bpp grayscale */
    PIXELFORMAT_G4       = FORMATTYPE_GRAYSCALE | 0x02, /* 4bpp grayscale */
    PIXELFORMAT_G8       = FORMATTYPE_GRAYSCALE | 0x03, /* 8bpp grayscale */
    PIXELFORMAT_AG88     = FORMATTYPE_GRAYSCALE | 0x13, /* 16bpp gray+alpha */
    PIXELFORMAT_GA88     = FORMATTYPE_GRAYSCALE | 0x23, /* 16bpp gray+alpha */
    PIXELFORMAT_PAL1     = FORMATTYPE_PALETTE   | 0x00, /* 1bpp palette */
    PIXELFORMAT_PAL2     = FORMATTYPE_PALETTE   | 0x01, /* 2bpp palette */
    PIXELFORMAT_PAL4     = FORMATTYPE_PALETTE   | 0x02, /* 4bpp palette */
    PIXELFORMAT_PAL8     = FORMATTYPE_PALETTE   | 0x03  /* 8bpp palette */
};

/* palette type */
enum paletteType {
    PALETTETYPE_AUTO = 0,     /* choose palette type automatically */
    PALETTETYPE_HLS  = 1,     /* HLS colorspace */
    PALETTETYPE_RGB  = 2      /* RGB colorspace */
};

/* policies of SIXEL encoding */
enum encodePolicy {
    ENCODEPOLICY_AUTO = 0,    /* choose encoding policy automatically */
    ENCODEPOLICY_FAST = 1,    /* encode as fast as possible */
    ENCODEPOLICY_SIZE = 2     /* encode to as small sixel sequence as possible */
};

/* method for re-sampling */
enum methodForResampling {
    RES_NEAREST  = 0,  /* Use nearest neighbor method */
    RES_GAUSSIAN = 1,  /* Use guaussian filter */
    RES_HANNING  = 2,  /* Use hanning filter */
    RES_HAMMING  = 3,  /* Use hamming filter */
    RES_BILINEAR = 4,  /* Use bilinear filter */
    RES_WELSH    = 5,  /* Use welsh filter */
    RES_BICUBIC  = 6,  /* Use bicubic filter */
    RES_LANCZOS2 = 7,  /* Use lanczos-2 filter */
    RES_LANCZOS3 = 8,  /* Use lanczos-3 filter */
    RES_LANCZOS4 = 9   /* Use lanczos-4 filter */
};
#endif
*/

#[allow(clippy::too_many_arguments)]
pub fn sixel_string(
    bytes: &[u8],
    width: i32,
    height: i32,
    pixelformat: PixelFormat,
    method_for_diffuse: DiffusionMethod,
    method_for_largest: MethodForLargest,
    method_for_rep: MethodForRep,
    quality_mode: Quality,
) -> SixelResult<String> {
    let mut sixel_data: Vec<u8> = Vec::new();

    let mut sixel_output = sixel_output::new(&mut sixel_data);
    sixel_output.set_encode_policy(EncodePolicy::AUTO);
    let mut sixel_dither = sixel_dither::new(256).unwrap();

    sixel_dither.set_optimize_palette(true);

    sixel_dither.initialize(
        bytes,
        width,
        height,
        pixelformat,
        method_for_largest,
        method_for_rep,
        quality_mode,
    )?;
    sixel_dither.set_pixelformat(pixelformat);
    sixel_dither.set_diffusion_type(method_for_diffuse);

    let mut bytes = bytes.to_vec();
    sixel_output.encode(&mut bytes, width, height, 0, &mut sixel_dither)?;

    Ok(String::from_utf8_lossy(&sixel_data).to_string())
} /*
  pub fn main() {
      let bytes = vec![
  ];

      println!("{}", sixel_string(&bytes, 128, 128, PixelFormat::RGB888, DiffusionMethod::Stucki, MethodForLargest::Auto, MethodForRep::Auto, Quality::AUTO).unwrap());
  }*/
