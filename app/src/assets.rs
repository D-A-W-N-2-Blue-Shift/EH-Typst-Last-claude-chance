// ============================================================================
// app/src/assets.rs — Assets visuels EMBARQUÉS dans le binaire
//
// Les PNG sont inclus à la compilation via include_bytes! : plus de dépendance
// à un chemin fichier au runtime (l'icône passait « une fois sur deux »). Les
// fichiers DOIVENT exister dans assets/ au build, sinon erreur de compilation
// claire (et non un échec silencieux au lancement).
//   - assets/icon.png    : icône d'application
//   - assets/core_bg.png : fond du hub core
// Le décodage PNG reste tolérant : si un asset est corrompu, log + fallback
// (icône système / pas de fond), jamais de panique.
// ============================================================================

/// Octets de l'icône, embarqués. include_bytes! échoue à la COMPILATION si le
/// fichier manque — exactement le comportement voulu (erreur claire).
const ICON_PNG: &[u8] = include_bytes!("../../assets/icon.png");
const CORE_BG_PNG: &[u8] = include_bytes!("../../assets/core_bg.png");

/// Décode un PNG embarqué en RGBA8 (octets, largeur, hauteur). None si illisible.
fn decode_rgba(bytes: &[u8], what: &str) -> Option<(Vec<u8>, u32, u32)> {
    match image::load_from_memory(bytes) {
        Ok(img) => {
            let rgba = img.into_rgba8();
            let (w, h) = rgba.dimensions();
            Some((rgba.into_raw(), w, h))
        }
        Err(e) => {
            tracing::warn!("Asset '{what}' non décodable ({e}). Fallback.");
            None
        }
    }
}

/// Icône de l'application. None ⇒ icône système par défaut (jamais de panique).
pub fn load_icon() -> Option<egui::IconData> {
    let (rgba, width, height) = decode_rgba(ICON_PNG, "icon.png")?;
    Some(egui::IconData {
        rgba,
        width,
        height,
    })
}

/// Image de fond du hub core en ColorImage egui prêt à texturer. None ⇒ pas de
/// fond (fallback : mieux rien que du rendu raté).
pub fn load_core_background() -> Option<egui::ColorImage> {
    let (rgba, w, h) = decode_rgba(CORE_BG_PNG, "core_bg.png")?;
    Some(egui::ColorImage::from_rgba_unmultiplied(
        [w as usize, h as usize],
        &rgba,
    ))
}
