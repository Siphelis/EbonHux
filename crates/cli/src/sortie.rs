use std::io::Write;

use ebonhux_coeur::erreur::{Erreur, Result};

const NOM_FLUX: &str = "<flux de sortie>";

/// Écrit une ligne ; un flux inutilisable est une erreur, jamais ignorée.
pub fn ligne(flux: &mut dyn Write, texte: &str) -> Result<()> {
    writeln!(flux, "{texte}").map_err(|cause| Erreur::io("écrire sur la sortie", NOM_FLUX, cause))
}
