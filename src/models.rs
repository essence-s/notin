//! Descarga de modelos de voz y VAD.
//!
//! Los modelos son de sherpa-onnx y se descargan de sus releases de GitHub.
//! Es el único momento en que la app necesita red: a partir de ahí funciona
//! completamente offline.
//!
//! Estructura en `~/Documentos/notin/models/`:
//! ```text
//! models/
//!   nemo-ctc-es/     model.int8.onnx  tokens.txt
//!   silero_vad/      silero_vad.onnx
//! ```

use std::fs;
use std::path::{Path, PathBuf};

/// Releases de los modelos de sherpa-onnx.
const RELEASES: &str = "https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models";

/// Nemo CTC (español, offline).
const NEMO_TARBALL: &str = "sherpa-onnx-nemo-fast-conformer-ctc-es-1424-int8.tar.bz2";
const NEMO_FILES: &[&str] = &["model.int8.onnx", "tokens.txt"];

/// VAD Silero (usado por todos los motores).
const SILERO_FILE: &str = "silero_vad.onnx";

/// Descarga (o completa) los modelos que necesita notin.
pub fn download_models(dir: &Path) -> Result<(), String> {
    let nemo_dir = dir.join("nemo-ctc-es");
    fs::create_dir_all(&nemo_dir)
        .map_err(|e| format!("no se pudo crear {}: {e}", nemo_dir.display()))?;

    if !nemo_dir.join("model.int8.onnx").exists() {
        let tmp = descarga_tarball(&format!("{RELEASES}/{NEMO_TARBALL}"), dir)?;
        copia(&tmp, &nemo_dir, NEMO_FILES)?;
        let _ = fs::remove_dir_all(&tmp);
    }

    let silero_dir = dir.join("silero_vad");
    fs::create_dir_all(&silero_dir)
        .map_err(|e| format!("no se pudo crear {}: {e}", silero_dir.display()))?;

    let silero_path = silero_dir.join(SILERO_FILE);
    if !silero_path.exists() {
        descarga_fichero(&format!("{RELEASES}/{SILERO_FILE}"), &silero_path)?;
    }

    println!("modelos listos en {}", dir.display());
    Ok(())
}

/// Descarga un tar.bz2 y lo extrae en `destino`. Devuelve la carpeta extraída.
fn descarga_tarball(url: &str, destino: &Path) -> Result<PathBuf, String> {
    let archivo = descarga(url)?;

    let salida = destino.join(".tmp-descarga");
    let _ = fs::remove_dir_all(&salida);
    fs::create_dir_all(&salida).map_err(|e| format!("no se pudo crear {salida:?}: {e}"))?;

    let file = fs::File::open(&archivo).map_err(|e| format!("no se pudo abrir {archivo:?}: {e}"))?;
    let bz = bzip2::read::BzDecoder::new(file);
    let mut ar = tar::Archive::new(bz);
    ar.unpack(&salida).map_err(|e| format!("no se pudo descomprimir el modelo: {e}"))?;
    let _ = fs::remove_file(&archivo);

    Ok(salida)
}

/// Descarga un fichero suelto a un fichero temporal y devuelve su ruta.
fn descarga(url: &str) -> Result<PathBuf, String> {
    let mut res = ureq::get(url)
        .call()
        .map_err(|e| format!("no se pudo descargar {url}: {e}"))?;

    let bytes = res
        .body_mut()
        .with_config()
        .limit(u64::MAX)
        .read_to_vec()
        .map_err(|e| format!("no se pudo leer {url}: {e}"))?;

    let nombre = url.rsplit('/').next().unwrap_or("modelo");
    let tmp = std::env::temp_dir().join(format!("notin-{nombre}-{}", std::process::id()));
    fs::write(&tmp, &bytes).map_err(|e| format!("no se pudo guardar la descarga: {e}"))?;
    Ok(tmp)
}

/// Descarga un fichero directamente a su destino.
fn descarga_fichero(url: &str, destino: &Path) -> Result<(), String> {
    let tmp = descarga(url)?;
    if let Some(d) = destino.parent() {
        fs::create_dir_all(d).map_err(|e| format!("no se pudo crear {:?}: {e}", d))?;
    }
    fs::copy(&tmp, destino).map_err(|e| format!("no se pudo guardar {destino:?}: {e}"))?;
    let _ = fs::remove_file(&tmp);
    Ok(())
}

/// Copia de un directorio extraído los ficheros que interesan.
fn copia(origen: &Path, destino: &Path, ficheros: &[&str]) -> Result<(), String> {
    fs::create_dir_all(destino)
        .map_err(|e| format!("no se pudo crear {:?}: {e}", destino))?;

    for nombre in ficheros {
        let origen_fichero = busca(origen, nombre)
            .ok_or_else(|| format!("el modelo no trae {nombre}"))?;
        fs::copy(&origen_fichero, destino.join(nombre))
            .map_err(|e| format!("no se pudo copiar {nombre}: {e}"))?;
        println!("  {nombre}");
    }
    Ok(())
}

/// Busca un fichero por nombre dentro de un directorio (recursivo).
fn busca(dir: &Path, nombre: &str) -> Option<PathBuf> {
    let entradas = fs::read_dir(dir).ok()?;
    for entrada in entradas.flatten() {
        let ruta = entrada.path();
        if ruta.is_dir() {
            if let Some(encontrado) = busca(&ruta, nombre) {
                return Some(encontrado);
            }
        } else if ruta.file_name().and_then(|n| n.to_str()) == Some(nombre) {
            return Some(ruta);
        }
    }
    None
}

/// Devuelve true si el modelo Nemo CTC y el VAD Silero están instalados.
pub fn model_installed(dir: &Path) -> bool {
    let nemo_dir = dir.join("nemo-ctc-es");
    let silero_path = dir.join("silero_vad").join(SILERO_FILE);
    nemo_dir.join("model.int8.onnx").exists()
        && nemo_dir.join("tokens.txt").exists()
        && silero_path.exists()
}

#[cfg(test)]
mod models_tests {
    use super::*;
    use std::fs;

    #[test]
    fn busca_encuentra_fichero_anidado() {
        let dir = std::env::temp_dir().join("notin_test_busca_anidado");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("sub").join("profunda")).expect("crea carpetas");
        fs::write(dir.join("sub").join("profunda").join("model.int8.onnx"), b"x").expect("escribe fichero");

        let encontrado = busca(&dir, "model.int8.onnx").expect("encuentra el fichero");
        assert_eq!(encontrado.file_name().unwrap(), "model.int8.onnx");

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn busca_no_encuentra_fichero_inexistente() {
        let dir = std::env::temp_dir().join("notin_test_busca_inexistente");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("crea carpeta");

        assert!(busca(&dir, "no_existe.onnx").is_none(), "no encuentra fichero que no existe");

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn models_model_installed_detects_installed() {
        let dir = std::env::temp_dir().join("notin_test_model_installed");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("nemo-ctc-es")).expect("crea carpeta nemo");
        fs::create_dir_all(dir.join("silero_vad")).expect("crea carpeta silero");
        fs::write(dir.join("nemo-ctc-es/model.int8.onnx"), b"x").expect("escribe modelo");
        fs::write(dir.join("nemo-ctc-es/tokens.txt"), b"x").expect("escribe tokens");
        fs::write(dir.join("silero_vad/silero_vad.onnx"), b"x").expect("escribe vad");

        assert!(model_installed(&dir), "detecta modelo instalado");

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn models_model_installed_detects_not_installed() {
        let dir = std::env::temp_dir().join("notin_test_model_not_installed");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("crea carpeta vacía");

        assert!(!model_installed(&dir), "no detecta modelo si falta");

        let _ = fs::remove_dir_all(&dir);
    }
}
