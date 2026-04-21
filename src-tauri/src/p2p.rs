use crc32fast::Hasher;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::io::Read;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::RwLock;

/// 50 MB per chunk
pub const CHUNK_SIZE: u64 = 50 * 1024 * 1024;

/// 單一檔案的區塊元資料
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkMeta {
    /// 區塊索引（0-based）
    pub index: u32,
    /// 區塊大小 (bytes)
    pub size: u64,
    /// CRC32 校驗碼
    pub crc32: u32,
}

/// 分享檔案的元資料
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SharedFileInfo {
    /// 唯一識別碼
    pub file_id: String,
    /// 原始檔名
    pub file_name: String,
    /// 檔案路徑（僅分享端使用）
    #[serde(skip_serializing)]
    pub file_path: String,
    /// 檔案總大小 (bytes)
    pub total_size: u64,
    /// 區塊總數
    pub chunk_count: u32,
    /// 各區塊的元資料（含 CRC32）
    pub chunks: Vec<ChunkMeta>,
}

/// P2P 端點狀態
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EndpointState {
    /// 端點 ID
    pub endpoint_id: String,
    /// 此端點目前正在下載哪個檔案（若有）
    pub file_id: Option<String>,
    /// 已擁有的區塊索引集合（key = file_id）
    pub owned_chunks: HashMap<String, HashSet<u32>>,
    /// 目前上傳連線數
    pub upload_count: u32,
    /// 目前下載連線數
    pub download_count: u32,
}

/// 全域的分享狀態（由中控中心管理）
pub type SharedState = Arc<RwLock<P2PState>>;

#[derive(Debug)]
pub struct P2PState {
    /// 所有可分享的檔案
    pub shared_files: Vec<SharedFileInfo>,
    /// 所有連線中的端點狀態 (key = endpoint_id)
    pub endpoints: HashMap<String, EndpointState>,
    /// 分享端本身的 WebRTC 端點 ID
    pub host_endpoint_id: String,
}

impl P2PState {
    pub fn new(host_endpoint_id: String) -> Self {
        // 建立 host 的初始端點狀態
        let host_state = EndpointState {
            endpoint_id: host_endpoint_id.clone(),
            file_id: None,
            owned_chunks: HashMap::new(),
            upload_count: 0,
            download_count: 0,
        };
        let mut endpoints = HashMap::new();
        endpoints.insert(host_endpoint_id.clone(), host_state);

        Self {
            shared_files: Vec::new(),
            endpoints,
            host_endpoint_id,
        }
    }

    /// 新增分享端的所有區塊到 host endpoint
    fn mark_host_owns_file(&mut self, file_id: &str, chunk_count: u32) {
        if let Some(host) = self.endpoints.get_mut(&self.host_endpoint_id) {
            let all_chunks: HashSet<u32> = (0..chunk_count).collect();
            host.owned_chunks.insert(file_id.to_string(), all_chunks);
        }
    }
}

impl Default for P2PState {
    fn default() -> Self {
        Self {
            shared_files: Vec::new(),
            endpoints: HashMap::new(),
            host_endpoint_id: String::new(),
        }
    }
}

/// 對指定檔案進行分塊並計算每塊 CRC32
pub fn process_file(file_path: &str) -> Result<SharedFileInfo, String> {
    let path = Path::new(file_path);
    let metadata = std::fs::metadata(path).map_err(|e| format!("無法讀取檔案: {e}"))?;
    let total_size = metadata.len();
    let file_name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "unknown".to_string());

    let chunk_count = if total_size == 0 {
        1
    } else {
        ((total_size + CHUNK_SIZE - 1) / CHUNK_SIZE) as u32
    };

    let mut file = std::fs::File::open(path).map_err(|e| format!("無法開啟檔案: {e}"))?;
    let mut chunks = Vec::with_capacity(chunk_count as usize);

    for index in 0..chunk_count {
        let remaining = total_size.saturating_sub(index as u64 * CHUNK_SIZE);
        let this_chunk_size = remaining.min(CHUNK_SIZE);

        let mut hasher = Hasher::new();
        let mut bytes_read: u64 = 0;
        let mut buf = vec![0u8; 64 * 1024]; // 64KB read buffer

        while bytes_read < this_chunk_size {
            let to_read = ((this_chunk_size - bytes_read) as usize).min(buf.len());
            let n = file
                .read(&mut buf[..to_read])
                .map_err(|e| format!("讀取區塊 {index} 時發生錯誤: {e}"))?;
            if n == 0 {
                break;
            }
            hasher.update(&buf[..n]);
            bytes_read += n as u64;
        }

        chunks.push(ChunkMeta {
            index,
            size: bytes_read,
            crc32: hasher.finalize(),
        });
    }

    let file_id = uuid::Uuid::new_v4().to_string();

    Ok(SharedFileInfo {
        file_id,
        file_name,
        file_path: file_path.to_string(),
        total_size,
        chunk_count,
        chunks,
    })
}

/// 將檔案加入分享狀態
pub async fn add_shared_file(state: &SharedState, file_path: &str) -> Result<SharedFileInfo, String> {
    let info = process_file(file_path)?;
    let mut s = state.write().await;

    // 避免重複加入同路徑的檔案
    if s.shared_files.iter().any(|f| f.file_path == file_path) {
        return Err("此檔案已在分享清單中".to_string());
    }

    let file_id = info.file_id.clone();
    let chunk_count = info.chunk_count;
    s.shared_files.push(info.clone());
    s.mark_host_owns_file(&file_id, chunk_count);

    Ok(info)
}

/// 從分享狀態移除檔案
pub async fn remove_shared_file(state: &SharedState, file_path: &str) {
    let mut s = state.write().await;
    if let Some(pos) = s.shared_files.iter().position(|f| f.file_path == file_path) {
        let file_id = s.shared_files[pos].file_id.clone();
        s.shared_files.remove(pos);
        // 清除各端點的相關區塊
        for ep in s.endpoints.values_mut() {
            ep.owned_chunks.remove(&file_id);
        }
    }
}

/// 讀取指定檔案的指定區塊資料（用於 Host 作為種子時透過 HTTP 提供）
pub async fn read_chunk_data(
    state: &SharedState,
    file_id: &str,
    chunk_index: u32,
) -> Result<Vec<u8>, String> {
    let s = state.read().await;
    let file_info = s
        .shared_files
        .iter()
        .find(|f| f.file_id == file_id)
        .ok_or_else(|| "找不到該檔案".to_string())?;

    let chunk_meta = file_info
        .chunks
        .get(chunk_index as usize)
        .ok_or_else(|| "無效的區塊索引".to_string())?;

    let file_path = file_info.file_path.clone();
    let offset = chunk_index as u64 * CHUNK_SIZE;
    let size = chunk_meta.size;

    drop(s); // 釋放讀鎖

    // 在 blocking thread 中讀取檔案
    tokio::task::spawn_blocking(move || {
        use std::io::{Read, Seek, SeekFrom};
        let mut file = std::fs::File::open(&file_path)
            .map_err(|e| format!("無法開啟檔案: {e}"))?;
        file.seek(SeekFrom::Start(offset))
            .map_err(|e| format!("無法定位: {e}"))?;
        let mut buf = vec![0u8; size as usize];
        file.read_exact(&mut buf)
            .map_err(|e| format!("讀取區塊資料失敗: {e}"))?;
        Ok(buf)
    })
    .await
    .map_err(|e| format!("spawn_blocking 失敗: {e}"))?
}
