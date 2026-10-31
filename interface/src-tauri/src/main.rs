//! Coque Tauri d'EbonHux : une fenêtre native qui affiche `App/interface/dist` et relaie
//! la commande unique `appeler` vers `ebonhux-service`.

mod logique;

use logique::{ErreurJs, Etat, FENETRE_PRINCIPALE};
use tauri::{AppHandle, Manager};

/// Commande unique de l'interface (`invoke('appeler', { commande, args })`). Le travail du service
/// peut durer (`tout_lancer` attend entre deux lancements) : il tourne hors du fil de la fenêtre.
#[tauri::command]
async fn appeler(etat: tauri::State<'_, Etat>, commande: String, args: serde_json::Value) -> Result<serde_json::Value, ErreurJs> {
    let etat = etat.inner().clone();
    tauri::async_runtime::spawn_blocking(move || etat.executer_commande(&commande, args))
        .await
        .unwrap_or_else(|erreur| Err(logique::erreur_tache(&erreur.to_string())))
}

/// Second lancement : la fenêtre existante passe au premier plan (les arguments du second processus
/// ne sont ni lus ni journalisés).
fn montrer_fenetre_existante(app: &AppHandle) {
    let Some(fenetre) = app.get_webview_window(FENETRE_PRINCIPALE) else {
        eprintln!("EbonHux : second lancement, mais la fenêtre principale est introuvable.");
        return;
    };
    for (action, resultat) in [("restaurer", fenetre.unminimize()), ("afficher", fenetre.show()), ("focaliser", fenetre.set_focus())] {
        if let Err(erreur) = resultat {
            eprintln!("EbonHux : impossible de {action} la fenêtre existante : {erreur}");
        }
    }
}

fn main() {
    let resultat = tauri::Builder::default()
        // Premier plugin : l'état des comptes lancés vit dans cette instance, une seconde la perdrait.
        .plugin(tauri_plugin_single_instance::init(|app, _arguments, _dossier| montrer_fenetre_existante(app)))
        .plugin(tauri_plugin_dialog::init())
        .manage(Etat::reel())
        .invoke_handler(tauri::generate_handler![appeler])
        .run(tauri::generate_context!());
    if let Err(erreur) = resultat {
        eprintln!("EbonHux : la fenêtre n'a pas pu démarrer : {erreur}");
        std::process::exit(1);
    }
}
