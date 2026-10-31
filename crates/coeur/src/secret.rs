use std::fmt;

use zeroize::Zeroize;

/// Texte à ne jamais montrer. Pas de `Clone` : une copie du clair doit être un geste explicite
/// (`Secret::new(autre.exposer().to_string())`), jamais un effet de bord.
pub struct Secret(String);

pub const MASQUE: &str = "***";

impl Secret {
    pub fn new(clair: String) -> Self {
        Self(clair)
    }

    /// Seul accès au clair.
    pub fn exposer(&self) -> &str {
        &self.0
    }

    pub fn est_vide(&self) -> bool {
        self.0.is_empty()
    }
}

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(MASQUE)
    }
}

impl fmt::Display for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(MASQUE)
    }
}

impl Drop for Secret {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}
