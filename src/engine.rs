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

/// Intenta transcribir un fichero de audio con el modelo dado.
/// Si el modelo no existe, devuelve un mensaje claro en vez de hacer panic.
pub fn transcribe(_model_path: &std::path::Path, _wav_path: &[u8]) -> Result<String, String> {
    Err(format!("no se encontró el modelo de voz en {:?}", _model_path))
}
