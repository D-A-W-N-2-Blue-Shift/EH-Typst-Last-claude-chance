// ============================================================================
// modules/editor/src/buffer.rs — Le buffer texte partagé
//
// - ropey::Rope : toutes les opérations per-frame sont O(log N) ou O(visible).
//   JAMAIS de rope.to_string() sur le thread UI.
// - Undo/Redo par TRANSACTIONS groupées (pas de snapshots) : une frappe et
//   sa mutation smart-typography (`--` → `—`) forment UNE transaction,
//   Ctrl+Z annule les deux ensemble. Les frappes consécutives coalescent.
// - BufferMap : déduplication par chemin canonique. Deux chemins (symlink
//   de 06_en_cours/ et original) qui pointent vers le même inode = UN seul
//   buffer en mémoire, partagé Arc<RwLock<…>> entre les fenêtres.
// - Sauvegarde : hash xxh3 du contenu disque pour distinguer une vraie
//   modification externe d'un événement notify parasite, + flag
//   "c'est moi qui écris" levé pendant la sauvegarde.
// ============================================================================

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, PoisonError, RwLock, RwLockReadGuard, RwLockWriteGuard};
use std::time::{Duration, Instant};

use ropey::Rope;

/// Une opération élémentaire, en indices de CARACTÈRES (pas d'octets).
#[derive(Debug, Clone)]
enum EditOp {
    Insert { at: usize, text: String },
    Delete { at: usize, text: String },
}

/// Une transaction = une unité d'annulation. Frappe + smart typography
/// + expansion de snippet déclenchée par la même touche = une transaction.
#[derive(Debug, Default)]
pub struct Transaction {
    ops: Vec<EditOp>,
    /// Position du curseur avant/après, pour le restaurer au undo/redo.
    pub cursor_before: usize,
    pub cursor_after: usize,
}

impl Transaction {
    pub fn is_empty(&self) -> bool {
        self.ops.is_empty()
    }
}

/// Pourquoi la dernière transaction a été ouverte — pilote la coalescence
/// (les frappes de texte consécutives fusionnent, le reste non).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TxnKind {
    /// Insertion de texte au fil de la frappe : coalesce avec la précédente
    /// si elle est adjacente et récente.
    Typing,
    /// Effacements consécutifs (Backspace/Suppr) : coalescent entre eux.
    Deleting,
    /// Tout le reste (coller, remplacer, snippet…) : transaction isolée.
    Other,
}

pub struct Buffer {
    pub rope: Rope,
    /// Chemin canonique (résolu, jamais de symlink résiduel).
    pub path: PathBuf,
    pub dirty: bool,
    /// Incrémenté à chaque mutation : les caches (coloration, layout,
    /// recherche, stats) comparent et s'invalident.
    pub version: u64,
    /// Journal borné des éditions (version, première ligne touchée).
    /// Chaque cache (il peut y en avoir plusieurs : deux fenêtres sur le
    /// même buffer) demande min_line_since(sa_version) et n'invalide que
    /// l'aval. Journal trop court = invalidation complète, jamais d'oubli.
    edits: std::collections::VecDeque<(u64, usize)>,
    undo_stack: Vec<Transaction>,
    redo_stack: Vec<Transaction>,
    pending: Option<(Transaction, TxnKind, Instant)>,
    /// xxh3 du contenu tel qu'il est (ou était) sur le disque.
    pub disk_hash: u64,
    pub last_save: Instant,
    /// Flag "c'est moi qui écris" : notify est ignoré jusqu'à cet instant.
    pub self_write_until: Option<Instant>,
    /// Une modification externe réelle a été détectée (hash différent).
    pub external_change: bool,
}

/// Fenêtre de coalescence des frappes : au-delà, nouvelle transaction.
const COALESCE_WINDOW: Duration = Duration::from_millis(1500);
/// Garde-fou : au-delà, on ne coalesce plus (un Ctrl+Z qui annule 3 pages
/// d'un coup n'aide personne).
const COALESCE_MAX_OPS: usize = 64;

impl Buffer {
    /// Charge le fichier. Le chemin DOIT déjà être canonique (BufferMap s'en
    /// charge) — assert en debug pour attraper les contournements.
    fn load(canonical: PathBuf) -> Result<Self, String> {
        let raw = std::fs::read_to_string(&canonical).map_err(|e| {
            format!(
                "Impossible d'ouvrir {} : {e}. Tu veux peut-être vérifier ton \
                 chmod ? Je te regarde.",
                canonical.display()
            )
        })?;
        let hash = xxhash_rust::xxh3::xxh3_64(raw.as_bytes());
        Ok(Self {
            rope: Rope::from_str(&raw),
            path: canonical,
            dirty: false,
            version: 0,
            edits: std::collections::VecDeque::new(),
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            pending: None,
            disk_hash: hash,
            last_save: Instant::now(),
            self_write_until: None,
            external_change: false,
        })
    }

    // -----------------------------------------------------------------------
    // Mutations. Toutes passent par insert/delete pour nourrir l'historique.
    // -----------------------------------------------------------------------

    /// Insère `text` à l'indice caractère `at`, dans la transaction courante.
    pub fn insert(&mut self, at: usize, text: &str) {
        if text.is_empty() {
            return;
        }
        let at = at.min(self.rope.len_chars());
        self.rope.insert(at, text);
        self.touch(self.rope.char_to_line(at));
        self.push_op(EditOp::Insert {
            at,
            text: to_owned(text),
        });
    }

    /// Supprime [from, to) (indices caractères), dans la transaction courante.
    pub fn delete(&mut self, from: usize, to: usize) {
        let len = self.rope.len_chars();
        let (from, to) = (from.min(len), to.min(len));
        if from >= to {
            return;
        }
        let removed = self.rope.slice(from..to).to_string();
        self.rope.remove(from..to);
        self.touch(self.rope.char_to_line(from));
        self.push_op(EditOp::Delete {
            at: from,
            text: removed,
        });
    }

    /// Remplace un range de lignes [from_line, to_line] (inclusif) par `new_text`.
    pub fn replace_line_range(&mut self, from_line: usize, to_line: usize, new_text: &str) {
        let total = self.rope.len_lines();
        let from_line = from_line.min(total.saturating_sub(1));
        let to_line = to_line.min(total.saturating_sub(1));
        if from_line > to_line {
            return;
        }
        let from_char = self.rope.line_to_char(from_line);
        let to_char = if to_line + 1 < total {
            self.rope.line_to_char(to_line + 1)
        } else {
            self.rope.len_chars()
        };
        self.delete(from_char, to_char);
        self.insert(from_char, new_text);
    }

    fn touch(&mut self, line: usize) {
        self.version += 1;
        self.dirty = true;
        self.edits.push_back((self.version, line));
        if self.edits.len() > 256 {
            self.edits.pop_front();
        }
    }

    /// Première ligne touchée depuis `version`. None = le journal ne remonte
    /// pas assez loin → le cache appelant invalide tout.
    pub fn min_line_since(&self, version: u64) -> Option<usize> {
        if version == self.version {
            return Some(usize::MAX); // rien de neuf
        }
        match self.edits.front() {
            Some((oldest, _)) if *oldest <= version + 1 => Some(
                self.edits
                    .iter()
                    .filter(|(v, _)| *v > version)
                    .map(|(_, l)| *l)
                    .min()
                    .unwrap_or(usize::MAX),
            ),
            _ => None,
        }
    }

    fn push_op(&mut self, op: EditOp) {
        match &mut self.pending {
            Some((txn, _, _)) => txn.ops.push(op),
            None => {
                // Mutation hors transaction explicite : transaction d'un op.
                self.pending = Some((
                    Transaction {
                        ops: vec![op],
                        cursor_before: 0,
                        cursor_after: 0,
                    },
                    TxnKind::Other,
                    Instant::now(),
                ));
            }
        }
    }

    // -----------------------------------------------------------------------
    // Transactions.
    // -----------------------------------------------------------------------

    /// Ouvre une transaction (ou continue la précédente si même `kind`
    /// Typing/Deleting, récente et pas trop grosse). `cursor` = position
    /// avant la mutation.
    pub fn begin_txn(&mut self, kind: TxnKind, cursor: usize) {
        let continue_pending = matches!(&self.pending, Some((txn, k, t))
            if *k == kind
                && kind != TxnKind::Other
                && t.elapsed() < COALESCE_WINDOW
                && txn.ops.len() < COALESCE_MAX_OPS);
        if !continue_pending {
            self.commit_txn();
            self.pending = Some((
                Transaction {
                    ops: Vec::new(),
                    cursor_before: cursor,
                    cursor_after: cursor,
                },
                kind,
                Instant::now(),
            ));
        } else if let Some((_, _, t)) = &mut self.pending {
            *t = Instant::now();
        }
    }

    /// Fin de la mutation courante : enregistre la position finale du
    /// curseur. La transaction reste ouverte pour coalescence éventuelle.
    pub fn end_txn(&mut self, cursor: usize) {
        if let Some((txn, _, _)) = &mut self.pending {
            txn.cursor_after = cursor;
        }
        self.redo_stack.clear();
    }

    /// Scelle la transaction en cours dans l'historique. Appelé avant un
    /// undo, une sauvegarde, ou l'ouverture d'une transaction non coalescée.
    pub fn commit_txn(&mut self) {
        if let Some((txn, _, _)) = self.pending.take() {
            if !txn.is_empty() {
                self.undo_stack.push(txn);
            }
        }
    }

    /// Annule la dernière transaction. Retourne la position curseur à
    /// restaurer, ou None si l'historique est vide.
    pub fn undo(&mut self) -> Option<usize> {
        self.commit_txn();
        let txn = self.undo_stack.pop()?;
        for op in txn.ops.iter().rev() {
            match op {
                EditOp::Insert { at, text } => {
                    self.rope.remove(*at..*at + text.chars().count());
                    self.touch(self.rope.char_to_line((*at).min(self.rope.len_chars())));
                }
                EditOp::Delete { at, text } => {
                    self.rope.insert(*at, text);
                    self.touch(self.rope.char_to_line(*at));
                }
            }
        }
        let cursor = txn.cursor_before;
        self.redo_stack.push(txn);
        Some(cursor.min(self.rope.len_chars()))
    }

    /// Rejoue la dernière transaction annulée.
    pub fn redo(&mut self) -> Option<usize> {
        let txn = self.redo_stack.pop()?;
        for op in txn.ops.iter() {
            match op {
                EditOp::Insert { at, text } => {
                    self.rope.insert(*at, text);
                    self.touch(self.rope.char_to_line(*at));
                }
                EditOp::Delete { at, text } => {
                    self.rope.remove(*at..*at + text.chars().count());
                    self.touch(self.rope.char_to_line((*at).min(self.rope.len_chars())));
                }
            }
        }
        let cursor = txn.cursor_after;
        self.undo_stack.push(txn);
        Some(cursor.min(self.rope.len_chars()))
    }

    // -----------------------------------------------------------------------
    // Disque.
    // -----------------------------------------------------------------------

    /// Sauvegarde sur disque. Lève le flag silence-notify AVANT d'écrire.
    pub fn save(&mut self, silence: Duration) -> Result<(), String> {
        self.commit_txn();
        self.self_write_until = Some(Instant::now() + silence);
        let text = self.rope.to_string(); // O(N) assumé : action explicite, pas per-frame.
        std::fs::write(&self.path, &text).map_err(|e| {
            format!(
                "Sauvegarde de {} ratée : {e}. Ton texte vit toujours en mémoire, \
                 mais le disque fait la sourde oreille.",
                self.path.display()
            )
        })?;
        self.disk_hash = xxhash_rust::xxh3::xxh3_64(text.as_bytes());
        self.dirty = false;
        self.external_change = false;
        self.last_save = Instant::now();
        Ok(())
    }

    /// Recharge depuis le disque (Ctrl+Shift+R). Vide l'historique : le
    /// contenu n'a plus de lien avec les transactions passées.
    pub fn reload(&mut self) -> Result<(), String> {
        let raw = std::fs::read_to_string(&self.path)
            .map_err(|e| format!("Rechargement de {} raté : {e}.", self.path.display()))?;
        self.rope = Rope::from_str(&raw);
        self.disk_hash = xxhash_rust::xxh3::xxh3_64(raw.as_bytes());
        self.dirty = false;
        self.external_change = false;
        self.version += 1;
        self.edits.clear(); // journal vide + version sautée = invalidation totale
        self.undo_stack.clear();
        self.redo_stack.clear();
        self.pending = None;
        Ok(())
    }

    /// Le fichier sur disque a-t-il VRAIMENT changé ? (notify a déclenché :
    /// on compare le hash pour écarter les faux positifs et nos propres
    /// écritures qui auraient échappé au flag silence.)
    pub fn disk_really_changed(&self) -> bool {
        match std::fs::read(&self.path) {
            Ok(bytes) => xxhash_rust::xxh3::xxh3_64(&bytes) != self.disk_hash,
            Err(_) => true, // supprimé/illisible = changement, à signaler.
        }
    }

    /// Le flag "c'est moi qui écris" est-il actif ?
    pub fn self_write_active(&self) -> bool {
        self.self_write_until.is_some_and(|t| Instant::now() < t)
    }
}

fn to_owned(s: &str) -> String {
    s.to_string()
}

// ===========================================================================
// BufferMap — déduplication par inode (chemin canonique).
// ===========================================================================

pub type SharedBuffer = Arc<RwLock<Buffer>>;

/// Accès au buffer partagé résilient à l'empoisonnement du verrou.
///
/// Doctrine §5/§6 : zéro déballage brutal. Un `panic` survenu pendant qu'un
/// thread tient le verrou empoisonne le `RwLock` ; un déballage brutal du
/// résultat de `read()`/`write()` ferait
/// alors paniquer EN CASCADE tous les accès suivants — un incident isolé
/// écroulerait l'application entière et le travail non sauvegardé. On récupère
/// le garde via `into_inner` : la donnée reste celle du buffer, l'édition se
/// poursuit au lieu de planter.
pub trait SharedBufferExt {
    fn read_buf(&self) -> RwLockReadGuard<'_, Buffer>;
    fn write_buf(&self) -> RwLockWriteGuard<'_, Buffer>;
}

impl SharedBufferExt for SharedBuffer {
    fn read_buf(&self) -> RwLockReadGuard<'_, Buffer> {
        self.read().unwrap_or_else(PoisonError::into_inner)
    }
    fn write_buf(&self) -> RwLockWriteGuard<'_, Buffer> {
        self.write().unwrap_or_else(PoisonError::into_inner)
    }
}

#[derive(Default)]
pub struct BufferMap {
    map: HashMap<PathBuf, SharedBuffer>,
}

/// Ouvre un buffer SANS passer par la map (tests, outils). La vraie vie
/// passe par BufferMap::open, qui déduplique.
pub fn open_standalone(path: &Path) -> Result<Buffer, String> {
    let canonical = std::fs::canonicalize(path)
        .map_err(|e| format!("Impossible de résoudre '{}' : {e}.", path.display()))?;
    Buffer::load(canonical)
}

impl BufferMap {
    /// Ouvre (ou retrouve) le buffer du fichier. Résout le chemin canonique
    /// AVANT d'allouer : symlink et original partagent le même buffer.
    pub fn open(&mut self, path: &Path) -> Result<(PathBuf, SharedBuffer), String> {
        let canonical = std::fs::canonicalize(path).map_err(|e| {
            format!(
                "Impossible de résoudre '{}' : {e}. Le fichier existait il y a \
                 une seconde, je te le jure.",
                path.display()
            )
        })?;
        if let Some(buf) = self.map.get(&canonical) {
            return Ok((canonical, Arc::clone(buf)));
        }
        let buf = Arc::new(RwLock::new(Buffer::load(canonical.clone())?));
        self.map.insert(canonical.clone(), Arc::clone(&buf));
        Ok((canonical, buf))
    }

    pub fn get(&self, canonical: &Path) -> Option<SharedBuffer> {
        self.map.get(canonical).cloned()
    }

    /// Libère les buffers que plus aucune fenêtre ne référence
    /// (strong count == 1 : seule la map les retient).
    pub fn drop_orphans(&mut self) {
        self.map.retain(|_, b| Arc::strong_count(b) > 1);
    }

    pub fn iter(&self) -> impl Iterator<Item = (&PathBuf, &SharedBuffer)> {
        self.map.iter()
    }
}

// ===========================================================================
// Tests.
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_buffer(text: &str) -> Result<Buffer, Box<dyn std::error::Error>> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("t.typ");
        std::fs::write(&path, text)?;
        let mut b = Buffer::load(std::fs::canonicalize(&path)?)?;
        // Le tempdir meurt à la fin du scope : on garde le texte en mémoire,
        // les tests ci-dessous ne retouchent pas le disque.
        b.path = PathBuf::from("/nonexistent/t.typ");
        Ok(b)
    }

    /// LE test exigé par le brief : symlink et original → un seul buffer.
    #[test]
    fn symlink_same_inode_same_buffer() -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempfile::tempdir()?;
        let original = dir.path().join("svetlana.typ");
        std::fs::write(&original, "Svetlana regarde la neige.")?;
        let link = dir.path().join("en_cours_svetlana.typ");
        std::os::unix::fs::symlink(&original, &link)?;

        let mut buffers = BufferMap::default();
        let (canon_a, buf_a) = buffers.open(&original)?;
        let (canon_b, buf_b) = buffers.open(&link)?;

        assert_eq!(canon_a, canon_b, "le symlink doit résoudre vers l'original");
        assert!(
            Arc::ptr_eq(&buf_a, &buf_b),
            "même inode = même buffer, pas deux"
        );

        // Modification via l'une, visible via l'autre.
        buf_a
            .write()
            .map_err(|_| "RwLock du buffer empoisonné")?
            .insert(0, "« ");
        assert!(buf_b
            .read()
            .map_err(|_| "RwLock du buffer empoisonné")?
            .rope
            .to_string()
            .starts_with("« "));
        Ok(())
    }

    #[test]
    fn undo_redo_restores_text_and_cursor() -> Result<(), Box<dyn std::error::Error>> {
        let mut b = temp_buffer("bonjour")?;
        b.begin_txn(TxnKind::Typing, 7);
        b.insert(7, " toi");
        b.end_txn(11);
        assert_eq!(b.rope.to_string(), "bonjour toi");

        let cur = b.undo().ok_or("undo devait restaurer un curseur")?;
        assert_eq!(b.rope.to_string(), "bonjour");
        assert_eq!(cur, 7);

        let cur = b.redo().ok_or("redo devait restaurer un curseur")?;
        assert_eq!(b.rope.to_string(), "bonjour toi");
        assert_eq!(cur, 11);
        Ok(())
    }

    /// Frappe + mutation smart typography = UNE seule annulation.
    #[test]
    fn typing_plus_typography_is_one_undo() -> Result<(), Box<dyn std::error::Error>> {
        let mut b = temp_buffer("a-")?;
        // L'utilisateur tape le second '-' ; la typo remplace "--" par "—".
        b.begin_txn(TxnKind::Typing, 2);
        b.insert(2, "-");
        b.delete(1, 3); // la conversion, DANS la même transaction
        b.insert(1, "—");
        b.end_txn(2);
        assert_eq!(b.rope.to_string(), "a—");

        b.undo();
        assert_eq!(
            b.rope.to_string(),
            "a-",
            "frappe + conversion annulées ensemble"
        );
        Ok(())
    }

    /// Les frappes consécutives coalescent ; un déplacement (Other) coupe.
    #[test]
    fn consecutive_typing_coalesces() -> Result<(), Box<dyn std::error::Error>> {
        let mut b = temp_buffer("")?;
        for (i, ch) in ["s", "v", "e", "t"].iter().enumerate() {
            b.begin_txn(TxnKind::Typing, i);
            b.insert(i, ch);
            b.end_txn(i + 1);
        }
        b.undo();
        assert_eq!(
            b.rope.to_string(),
            "",
            "une rafale de frappes = un seul undo"
        );
        Ok(())
    }

    #[test]
    fn delete_then_undo() -> Result<(), Box<dyn std::error::Error>> {
        let mut b = temp_buffer("la neige tombe")?;
        b.begin_txn(TxnKind::Other, 0);
        b.delete(3, 9); // "neige "
        b.end_txn(3);
        assert_eq!(b.rope.to_string(), "la tombe");
        b.undo();
        assert_eq!(b.rope.to_string(), "la neige tombe");
        Ok(())
    }

    #[test]
    fn redo_cleared_by_new_edit() -> Result<(), Box<dyn std::error::Error>> {
        let mut b = temp_buffer("x")?;
        b.begin_txn(TxnKind::Typing, 1);
        b.insert(1, "y");
        b.end_txn(2);
        b.undo();
        b.begin_txn(TxnKind::Other, 1);
        b.insert(1, "z");
        b.end_txn(2);
        assert!(b.redo().is_none(), "une édition après undo vide le redo");
        assert_eq!(b.rope.to_string(), "xz");
        Ok(())
    }
}
