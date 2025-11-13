use wasm_bindgen::prelude::*;
use js_sys::Float32Array;

// --- 定数 ---

/// RMS計算時の最小値。0による対数計算エラー（-Infinity）を防ぐための微小な値 (epsilon)
const MIN_RMS: f32 = 1.0e-10; // -100 dBFS に相当

// --- ユーティリティ ---

/// アプリケーション起動時に一度だけ呼び出す初期化関数
/// これにより、Rust側でpanicが発生した場合、ブラウザのコンソールにエラー内容が出力されます。
#[wasm_bindgen]
pub fn init_panic_hook() {
    #[cfg(feature = "console_error_panic_hook")]
    console_error_panic_hook::set_once();
}

// --- コアロジック ---

/**
 * 指定された音声チャンク（サンプルのスライス）のRMS（二乗平均平方根）を計算します。
 *
 * @param chunk 音声データのチャンク
 * @returns このチャンクのRMS値
 */
fn calculate_rms(chunk: &[f32]) -> f32 {
    if chunk.is_empty() {
        return 0.0;
    }

    // 各サンプルの二乗和を計算
    // f64 (倍精度) を使用して、合計時の精度低下やオーバーフローを防ぐ
    let sum_sq: f64 = chunk.iter().map(|&sample| (sample as f64) * (sample as f64)).sum();

    // 二乗和の平均を計算し、その平方根（RMS）をとる
    // (sum_sq / chunk.len() as f64) が分散に相当
    ((sum_sq / chunk.len() as f64).sqrt()) as f32
}

/**
 * RMS値をdBFS (Decibels relative to full scale) に変換します。
 *
 * @param rms RMS値
 * @returns dBFS値 (例: -60.0)
 */
fn rms_to_dbfs(rms: f32) -> f32 {
    // 0除算を避けるため、MIN_RMSと比較して大きい方をとる
    let rms_clamped = rms.max(MIN_RMS);

    // 20 * log10(rms) でデシベルに変換
    // PCMデータは既に -1.0 ～ 1.0 の範囲（Full Scale）と仮定
    20.0 * rms_clamped.log10()
}

// --- WASM公開関数 ---

/**
 * Web Workerから呼び出されるメイン関数。
 * PCMデータ全体を受け取り、指定されたチャンクサイズで分割しながら
 * 各チャンクの音量 (dBFS) を計算し、結果の配列を返します。
 *
 * @param pcm_data - デコード済みの生音声データ (Float32Array)
 * @param chunk_size_samples - 1チャンクあたりのサンプル数 (例: 480サンプル)
 * @returns 各チャンクのdBFS値が格納された Float32Array
 */
#[wasm_bindgen(js_name = analyzeAudioRms)]
pub fn analyze_audio_rms(pcm_data: Float32Array, chunk_size_samples: usize) -> Float32Array {
    // JSのFloat32ArrayをRustのスライスに変換（コピーが発生するが、
    // WASM境界を越えるため、また安全なスライス操作のために許容する）
    // 大容量データの場合、Web WorkerからWASMのメモリに直接書き込む高度な手法もあるが、
    // まずは堅牢な実装とする。
    let pcm_vec: Vec<f32> = pcm_data.to_vec();

    // pcm_data.length() を使うよりもRust側で長さを取得する方が安全
    let total_samples = pcm_vec.len();

    if total_samples == 0 || chunk_size_samples == 0 {
        // 空の配列を返す
        return Float32Array::new_with_length(0);
    }

    // 結果を格納する配列のサイズを計算 (切り上げ)
    let num_chunks = (total_samples + chunk_size_samples - 1) / chunk_size_samples;
    let mut dbfs_results: Vec<f32> = Vec::with_capacity(num_chunks);

    // pcm_vecを不変スライスとして取得
    let pcm_slice = pcm_vec.as_slice();

    // 指定されたチャンクサイズでイテレーション
    // Rustの `chunks_exact` と `last` を組み合わせて効率的に処理
    let mut chunks_iter = pcm_slice.chunks_exact(chunk_size_samples);

    for chunk in &mut chunks_iter {
        let rms = calculate_rms(chunk);
        dbfs_results.push(rms_to_dbfs(rms));
    }

    // 最後のチャンク（chunk_size_samplesに満たない余り）を処理
    let remainder = chunks_iter.remainder();
    if !remainder.is_empty() {
        let rms = calculate_rms(remainder);
        dbfs_results.push(rms_to_dbfs(rms));
    }

    // RustのVec<f32>からJSのFloat32Arrayに変換して返す (コピーが発生)
    // `from` は効率的な変換（コピー）を提供します。
    Float32Array::from(dbfs_results.as_slice())
}
