pub const BACKGROUND_JPG: &[u8] = include_bytes!("../assets/background.jpg");
pub const QR_OVERLAY_PNG: &[u8] = include_bytes!("../assets/qr-overlay.png");
pub const DEJAVU_SANS_TTF: &[u8] = include_bytes!("../assets/fonts/DejaVuSans.ttf");
pub const DEJAVU_SANS_MONO_TTF: &[u8] = include_bytes!("../assets/fonts/DejaVuSansMono.ttf");
pub const MONASPACE_KRYPTON_TTF: &[u8] =
    include_bytes!("../assets/fonts/MonaspaceKryptonVarVF.ttf");
pub static APP_FONTS: &[&[u8]] = &[DEJAVU_SANS_TTF, MONASPACE_KRYPTON_TTF, DEJAVU_SANS_MONO_TTF];
