// ============================================================================
// app_nexus/src/assets.rs — Assets visuels EMBARQUÉS dans le binaire
//
// Même mécanisme éprouvé que l'app écrivain (app/src/assets.rs, archivée) :
// PNG inclus à la compilation via include_bytes! — plus de dépendance à un
// chemin fichier au runtime. Les fichiers DOIVENT exister dans assets/ au
// build, sinon erreur de compilation claire (et non un échec silencieux au
// lancement).
//   - assets/Hive-RBMK-icone.png    : icône d'application
//   - assets/Hive-RBMK-bck_core.png : fond de la fenêtre core
// Le décodage PNG reste tolérant : si un asset est corrompu, log + fallback
// (icône système / pas de fond), jamais de panique.
// ============================================================================

/// Octets de l'icône, embarqués. include_bytes! échoue à la COMPILATION si
/// le fichier manque — exactement le comportement voulu (erreur claire).
const ICON_PNG: &[u8] = include_bytes!("../../assets/Hive-RBMK-icone.png");
const CORE_BG_PNG: &[u8] = include_bytes!("../../assets/Hive-RBMK-bck_core.png");

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

/// Côté maximal de l'icône transmise au serveur de fenêtres. L'icône passe
/// dans UNE requête X11 (`_NET_WM_ICON`, RGBA brut) : un PNG source de
/// 2048×2048 = 16 MiB dépasse la longueur maximale de requête et fait
/// échouer la création de la fenêtre (« Maximum request length exceeded »,
/// constaté au lancement réel). 256×256 = 256 KiB, standard des icônes de
/// fenêtre, toujours accepté.
const ICON_MAX_SIDE: u32 = 256;

/// Icône de l'application, bornée à ICON_MAX_SIDE. None ⇒ icône système
/// par défaut (jamais de panique).
pub fn load_icon() -> Option<egui::IconData> {
    let img = match image::load_from_memory(ICON_PNG) {
        Ok(img) => img,
        Err(e) => {
            tracing::warn!("Asset 'Hive-RBMK-icone.png' non décodable ({e}). Fallback.");
            return None;
        }
    };
    let img = if img.width() > ICON_MAX_SIDE || img.height() > ICON_MAX_SIDE {
        img.thumbnail(ICON_MAX_SIDE, ICON_MAX_SIDE)
    } else {
        img
    };
    let rgba = img.into_rgba8();
    let (width, height) = rgba.dimensions();
    Some(egui::IconData {
        rgba: rgba.into_raw(),
        width,
        height,
    })
}

/// Image de fond du core en ColorImage egui prêt à texturer. None ⇒ pas de
/// fond (fallback : mieux rien que du rendu raté).
pub fn load_core_background() -> Option<egui::ColorImage> {
    let (rgba, w, h) = decode_rgba(CORE_BG_PNG, "Hive-RBMK-bck_core.png")?;
    Some(egui::ColorImage::from_rgba_unmultiplied(
        [w as usize, h as usize],
        &rgba,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn les_deux_assets_embarques_se_decodent() {
        assert!(load_icon().is_some(), "icône embarquée non décodable");
        assert!(
            load_core_background().is_some(),
            "fond embarqué non décodable"
        );
    }

    #[test]
    fn licone_est_bornee_a_la_taille_max_requete_x11() {
        let icon = load_icon().expect("icône décodable");
        assert!(
            icon.width <= ICON_MAX_SIDE && icon.height <= ICON_MAX_SIDE,
            "icône {}x{} : dépasserait la longueur max d'une requête X11",
            icon.width,
            icon.height
        );
    }
}
