//! The bundled fonts (assets/fonts, with their licenses): Inter for the
//! interface, JetBrains Mono for times. Their family names are the
//! `rc.font.ui` and `rc.font.mono` tokens.

use std::sync::Arc;

use zgx_desktop::FontData;

const FACES: [&[u8]; 4] = [
    include_bytes!("../assets/fonts/Inter-Regular.ttf"),
    include_bytes!("../assets/fonts/Inter-Medium.ttf"),
    include_bytes!("../assets/fonts/Inter-SemiBold.ttf"),
    include_bytes!("../assets/fonts/JetBrainsMono-Regular.ttf"),
];

pub fn bundled() -> impl Iterator<Item = Result<FontData, Box<dyn std::error::Error>>> {
    FACES.into_iter().map(|bytes| FontData::new(Arc::<[u8]>::from(bytes)).map_err(Into::into))
}

#[cfg(test)]
mod tests {
    #[test]
    fn every_face_loads() {
        for face in super::bundled() {
            face.unwrap();
        }
    }
}
