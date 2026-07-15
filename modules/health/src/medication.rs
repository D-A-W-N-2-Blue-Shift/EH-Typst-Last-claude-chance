// ============================================================================
// modules/health/src/medication.rs — Sous-vue Médication (doc §5.3)
//
// Registre de référence : table nexus_db.medications (résolution de
// l'ambiguïté doc §3 vs §5.3/§7 — voir README_MODULE.md « Décision
// structurelle »). Pas de courbe théorique : heure(s) de prise du jour +
// historique 7 jours seulement. Ressenti différé : champ disponible 3h
// après la prise, activable/désactivable via la section "health" de
// engram.ron (config.rs) — doc §5.3 « configurable off ».
// ============================================================================

use chrono::{NaiveDate, NaiveDateTime};

/// Une prise a-t-elle besoin d'un ressenti ? Vrai si aucun ressenti n'est
/// encore enregistré ET que 3h se sont écoulées depuis la prise. Un
/// timestamp illisible ne déclenche jamais de prompt (jamais de panique sur
/// une donnée corrompue).
pub fn needs_ressenti_prompt(dose: &nexus_db::MedDose, now: NaiveDateTime) -> bool {
    if dose.ressenti.is_some() {
        return false;
    }
    let Some(taken_at) = parse_taken_at(&dose.taken_at) else {
        return false;
    };
    now.signed_duration_since(taken_at) >= chrono::Duration::hours(3)
}

fn parse_taken_at(raw: &str) -> Option<NaiveDateTime> {
    NaiveDateTime::parse_from_str(raw, "%Y-%m-%dT%H:%M:%S").ok()
}

fn dose_date(dose: &nexus_db::MedDose) -> Option<NaiveDate> {
    let date_part = dose.taken_at.split('T').next().unwrap_or(&dose.taken_at);
    NaiveDate::parse_from_str(date_part, "%Y-%m-%d").ok()
}

/// Prises dont la date correspond à `today` (doc : « heure(s) de prise du jour »).
pub fn doses_on(doses: &[nexus_db::MedDose], day: NaiveDate) -> Vec<&nexus_db::MedDose> {
    doses.iter().filter(|d| dose_date(d) == Some(day)).collect()
}

/// Prises des `window_days` derniers jours (bornes incluses) depuis `as_of`
/// (doc : « historique 7 jours »).
pub fn doses_in_window<'a>(
    doses: &'a [nexus_db::MedDose],
    as_of: NaiveDate,
    window_days: i64,
) -> Vec<&'a nexus_db::MedDose> {
    let cutoff = as_of - chrono::Duration::days(window_days.max(1) - 1);
    doses
        .iter()
        .filter(|d| dose_date(d).is_some_and(|dt| dt >= cutoff && dt <= as_of))
        .collect()
}

/// État du mini-formulaire « ajouter un médicament » (registre de référence).
#[derive(Default)]
pub struct MedicationForm {
    pub nom: String,
    pub molecule: String,
    pub dose_default: f32,
    pub notes: String,
}

/// État du popup « Prise » (bouton par médicament dans la sous-vue) : ne
/// PAS confondre avec Redrop — ici le timestamp est éditable (doc §5.3),
/// chez Redrop il est toujours `now()` (doc §6).
pub struct DoseForm {
    pub med_id: String,
    pub timestamp_input: String,
    pub dose_mg: f32,
}

impl DoseForm {
    pub fn new(med: &nexus_db::Medication, now: NaiveDateTime) -> Self {
        Self {
            med_id: med.id.clone(),
            timestamp_input: now.format("%Y-%m-%d %H:%M").to_string(),
            dose_mg: med.dose_default as f32,
        }
    }

    /// Parse le timestamp saisi ("YYYY-MM-DD HH:MM") vers la forme stockée
    /// en base ("YYYY-MM-DDTHH:MM:SS"). `None` si illisible — la saisie
    /// reste affichée pour correction, jamais silencieusement ignorée.
    pub fn parsed_taken_at(&self) -> Option<String> {
        NaiveDateTime::parse_from_str(self.timestamp_input.trim(), "%Y-%m-%d %H:%M")
            .ok()
            .map(|dt| dt.format("%Y-%m-%dT%H:%M:%S").to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dose(taken_at: &str, ressenti: Option<i64>) -> nexus_db::MedDose {
        nexus_db::MedDose {
            id: taken_at.to_string(),
            med_id: "m1".into(),
            taken_at: taken_at.to_string(),
            dose_mg: 20.0,
            ressenti,
            notes: None,
        }
    }

    fn dt(s: &str) -> NaiveDateTime {
        NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S").expect("datetime de test valide")
    }

    #[test]
    fn pas_de_prompt_si_ressenti_deja_present() {
        let d = dose("2026-07-12T09:00:00", Some(4));
        assert!(!needs_ressenti_prompt(&d, dt("2026-07-12T20:00:00")));
    }

    #[test]
    fn pas_de_prompt_avant_3h() {
        let d = dose("2026-07-12T09:00:00", None);
        assert!(!needs_ressenti_prompt(&d, dt("2026-07-12T11:30:00")));
    }

    #[test]
    fn prompt_apres_3h_pile() {
        let d = dose("2026-07-12T09:00:00", None);
        assert!(needs_ressenti_prompt(&d, dt("2026-07-12T12:00:00")));
    }

    #[test]
    fn pas_de_prompt_si_timestamp_illisible() {
        let d = nexus_db::MedDose {
            id: "x".into(),
            med_id: "m1".into(),
            taken_at: "pas-un-timestamp".into(),
            dose_mg: 20.0,
            ressenti: None,
            notes: None,
        };
        assert!(!needs_ressenti_prompt(&d, dt("2026-07-12T20:00:00")));
    }

    #[test]
    fn doses_on_filtre_par_date_exacte() {
        let doses = vec![
            dose("2026-07-11T09:00:00", None),
            dose("2026-07-12T08:00:00", None),
            dose("2026-07-12T20:00:00", None),
        ];
        let today = doses_on(
            &doses,
            NaiveDate::parse_from_str("2026-07-12", "%Y-%m-%d").expect("date"),
        );
        assert_eq!(today.len(), 2);
    }

    #[test]
    fn doses_in_window_respecte_les_bornes() {
        let doses = vec![
            dose("2026-07-01T09:00:00", None), // hors fenêtre 7 jours
            dose("2026-07-06T09:00:00", None), // dans la fenêtre
            dose("2026-07-12T09:00:00", None),
        ];
        let as_of = NaiveDate::parse_from_str("2026-07-12", "%Y-%m-%d").expect("date");
        let recent = doses_in_window(&doses, as_of, 7);
        assert_eq!(recent.len(), 2);
    }

    #[test]
    fn dose_form_parse_timestamp_valide() {
        let med = nexus_db::Medication {
            id: "m1".into(),
            nom: "Test".into(),
            molecule: "test".into(),
            dose_default: 20.0,
            notes: None,
        };
        let mut form = DoseForm::new(&med, dt("2026-07-12T09:00:00"));
        assert_eq!(
            form.parsed_taken_at(),
            Some("2026-07-12T09:00:00".to_string())
        );
        form.timestamp_input = "pas une date".into();
        assert_eq!(form.parsed_taken_at(), None);
    }
}
