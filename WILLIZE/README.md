# WILLIZE — moteur d'orchestration appris sur John Williams

Rust, sans dépendance. Apprend sur **tous les MIDI du dépôt (sauf piano)** puis arrange n'importe quelle idée : thème, ostinato, accords, basse.

## Ce qu'il apprend (`ANALYSE_WILLIAMS.md`)
- **Harmonie** : accords et enchaînements par degré (majeur/mineur), rythme harmonique, probabilité de changer d'accord au 1er temps ou en milieu de mesure, renversements, pédales.
- **Mélodie / accord** : quelle note de l'accord porte la mélodie (temps fort/faible).
- **Lignes** : intervalles et durées des thèmes et des contre-chants, complémentarité, mouvement contraire/oblique.
- **Orchestration** : pour chaque porteur du thème (cors, violons, trompettes, bois…) et chaque nuance, probabilité que chaque pupitre double (et à quelle octave), tienne, fasse un ostinato, un contre-chant ou la basse.
- **Écriture** : tessitures par pupitre et par rôle, nombre de voix, espacement des voix, figures rythmiques par pupitre.
- **Forme** : mesures selon l'ambiance, changements de mesure (mesures isolées 2/4, 3/4…), modulations, ouvertures, roulements de timbales, cymbales et grosse caisse.

## Utilisation
```
cd willize && cargo build --release
./target/release/willize arrange ../test.mid --track 6horns --mp3 ../WILLIZE.mp3
```
Options : `--mood majestic|lyrical|action|mysterious` · `--seconds 60` · `--bpm` · `--key Em` · `--hr 0.8` (harmonie plus lente) · `--carrier horn|violin|trumpet|oboe…` · `--modulate` · `--add-theme` (nouveau thème par-dessus ton matériau) · `--track NOM` (répétable).

Ton MIDI peut contenir plusieurs pistes ; chacune est classée automatiquement :
- **thème** : ligne mélodique ;
- **ostinato** : motif répété ; il est conservé tel quel et le moteur peut composer un thème par-dessus ;
- **accords** : ils guident l'harmonisation ;
- **basse**.

## Palette
- **Cordes** : Violons 1, Violons 2, Altos, Violoncelles, Contrebasses (une voix chacun).
- **Bois** : Flûtes 2, Piccolo 1, Hautbois 2, Clarinettes 2, Bassons 2.
- **Cuivres** : Cors 4, Trompettes 3, Trombones 3, Tuba 1.
- **Percussions** : Timbales, Grosse caisse, Piatti.
- **Harpe, claviers et lames** : Harpe, Glockenspiel, Célesta, Xylophone, Marimba.

Sortie : MIDI multipiste (dynamique en CC1, signatures, tempo) et, en option, un rendu MP3 GM de contrôle.
