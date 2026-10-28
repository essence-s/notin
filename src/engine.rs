use std::ffi::{CStr, CString};
use std::path::Path;

use sherpa_rs::sherpa_rs_sys as sys;

/// Intenta transcribir muestras de audio (f32, 16 kHz mono) con el modelo dado.
/// Si el modelo no existe, devuelve un mensaje claro en vez de hacer panic.
pub fn transcribe(model_path: &Path, samples: &[f32]) -> Result<String, String> {
    if !model_path.exists() {
        return Err(format!(
            "no se encontró el modelo de voz en {:?}",
            model_path
        ));
    }
    let tokens_path = model_path.with_file_name("tokens.txt");
    if !tokens_path.exists() {
        return Err(format!(
            "no se encontró el fichero de tokens en {:?}",
            tokens_path
        ));
    }

    let model = CString::new(model_path.as_os_str().as_encoded_bytes())
        .map_err(|e| format!("ruta de modelo inválida: {e}"))?;
    let tokens = CString::new(tokens_path.as_os_str().as_encoded_bytes())
        .map_err(|e| format!("ruta de tokens inválida: {e}"))?;
    let provider = CString::new("cpu").unwrap();
    let decoding = CString::new("greedy_search").unwrap();

    let model_config = sys::SherpaOnnxOfflineModelConfig {
        nemo_ctc: sys::SherpaOnnxOfflineNemoEncDecCtcModelConfig {
            model: model.as_ptr(),
        },
        tokens: tokens.as_ptr(),
        num_threads: 4,
        debug: 0,
        provider: provider.as_ptr(),
        ..unsafe { std::mem::zeroed() }
    };

    let config = sys::SherpaOnnxOfflineRecognizerConfig {
        feat_config: sys::SherpaOnnxFeatureConfig {
            sample_rate: 16_000,
            feature_dim: 80,
        },
        model_config,
        decoding_method: decoding.as_ptr(),
        max_active_paths: 4,
        ..unsafe { std::mem::zeroed() }
    };

    let recognizer = unsafe { sys::SherpaOnnxCreateOfflineRecognizer(&config) };
    if recognizer.is_null() {
        return Err("no se pudo crear el reconocedor nemo_ctc".to_string());
    }

    let texto = if samples.is_empty() {
        String::new()
    } else {
        unsafe {
            let stream = sys::SherpaOnnxCreateOfflineStream(recognizer);
            if stream.is_null() {
                sys::SherpaOnnxDestroyOfflineRecognizer(recognizer);
                return Err("no se pudo crear el stream de decodificación".to_string());
            }
            sys::SherpaOnnxAcceptWaveformOffline(
                stream,
                16_000,
                samples.as_ptr(),
                samples.len() as i32,
            );
            sys::SherpaOnnxDecodeOfflineStream(recognizer, stream);
            let result = sys::SherpaOnnxGetOfflineStreamResult(stream);
            let text = if result.is_null() {
                String::new()
            } else {
                let text = CStr::from_ptr((*result).text)
                    .to_string_lossy()
                    .into_owned();
                sys::SherpaOnnxDestroyOfflineRecognizerResult(result);
                text
            };
            sys::SherpaOnnxDestroyOfflineStream(stream);
            text
        }
    };

    unsafe { sys::SherpaOnnxDestroyOfflineRecognizer(recognizer) };
    Ok(texto)
}

// Prueba 3 del par: `missing_model_returns_clear_message_instead_of_crash`.
// Dado una ruta de modelo inexistente, cuando se intenta transcribir, entonces
// devuelve un error con mensaje claro y no hace panic.
#[cfg(test)]
mod engine_tests {
    #[test]
    fn missing_model_returns_clear_message() {
        let model_path = std::env::temp_dir().join("no_existe_modelo.onnx");
        let _ = std::fs::remove_file(&model_path);
        let result = crate::engine::transcribe(&model_path, &[]);
        assert!(
            result.is_err(),
            "transcribir sin modelo debe devolver error, no panic"
        );
        let err = result.unwrap_err();
        assert!(!err.is_empty(), "el mensaje de error no debe estar vacío");
        let msg = err.to_lowercase();
        assert!(
            msg.contains("modelo") || msg.contains("model") || msg.contains("no encontrado") || msg.contains("no existe"),
            "el error debe mencionar claramente el modelo faltante: {err}"
        );
    }
}
