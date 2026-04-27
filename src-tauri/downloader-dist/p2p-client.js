// ── CRC32 計算工具 ──
const CRC32 = (() => {
  const table = new Uint32Array(256);
  for (let i = 0; i < 256; i++) {
    let c = i;
    for (let j = 0; j < 8; j++) c = c & 1 ? 0xEDB88320 ^ (c >>> 1) : c >>> 1;
    table[i] = c;
  }
  return {
    init() {
      return 0xFFFFFFFF;
    },
    update(crc, data) {
      const u8 = data instanceof Uint8Array ? data : new Uint8Array(data);
      let c = crc >>> 0;
      for (let i = 0; i < u8.length; i++) c = table[(c ^ u8[i]) & 0xFF] ^ (c >>> 8);
      return c >>> 0;
    },
    finalize(crc) {
      return (crc ^ 0xFFFFFFFF) >>> 0;
    },
    compute(data) {
      const crc = this.update(this.init(), data);
      return this.finalize(crc);
    }
  };
})();

// 只在「無進度」時中斷，持續收到資料就會延長。
const WEBRTC_CHUNK_IDLE_TIMEOUT_MS = 30000;
const WEBRTC_TIMEOUT_CHECK_INTERVAL_MS = 1000;
const CHUNK_SIZE = 50 * 1024 * 1024;

// ── P2P 下載管理器 ──
class P2PDownloader {
  constructor() {
    this.endpointId = crypto.randomUUID();
    this.ws = null;
    this.files = [];
    this.activeDownload = null; // { fileId, fileInfo, ownedChunks: Set, fileHandle, writable, totalChunks }
    this.peerConnections = new Map(); // peerId -> RTCPeerConnection
    this.uploadCount = 0;
    this.downloadCount = 0;
    this.webrtcDownloadCount = 0;
    this.pendingRequests = new Set(); // chunk indices currently being requested/downloaded
    this.onStateChange = null;
    this.onLog = null;
    this.hostEndpointId = null;
    this.maxConcurrentDownloads = 3;
    this.completedFiles = new Set(); // 已完成下載的 fileId
    this.downloadQueue = []; // { fileId, fileHandle, totalChunks, ... }
    this.fileHandles = new Map(); // fileId -> FileSystemFileHandle
    this.chunkBuffers = new Map(); // fileId -> { chunkIndex -> Uint8Array } (已完成 chunk 的記憶體快取)
    this.httpInFlight = new Set(); // `${fileId}:${chunkIndex}`
    this.httpQueue = []; // [{ fileId, chunkIndex, sourcePeer }]
    // 每個 fileId 的 HTTP / WebRTC 下載成功區塊數
    this.chunkStats = new Map(); // fileId -> { http: number, webrtc: number, upload: number }
  }

  _formatLogTimestamp() {
    const now = new Date();
    const hh = String(now.getHours()).padStart(2, '0');
    const mm = String(now.getMinutes()).padStart(2, '0');
    const ss = String(now.getSeconds()).padStart(2, '0');
    const mmm = String(now.getMilliseconds()).padStart(3, '0');
    return `${hh}:${mm}:${ss}.${mmm}`;
  }

  log(msg) {
    const local = this.endpointId ? this.endpointId.slice(0, 8) : 'unknown';
    const formatted = `[${this._formatLogTimestamp()}][${local}] ${msg}`;
    console.log(`[P2P] ${formatted}`);
  }

  uiLog(msg) {
    if (this.onLog) this.onLog(msg);
  }

  // ── WebSocket 連線 ──
  connect() {
    const protocol = location.protocol === 'https:' ? 'wss:' : 'ws:';
    const url = `${protocol}//${location.host}/ws`;
    this.ws = new WebSocket(url);

    this.ws.onopen = () => {
      this.log('WebSocket 已連線');
      this.ws.send(JSON.stringify({ type: 'register', endpoint_id: this.endpointId }));
      this._startStatusReporting();
      this._notifyStateChange();
    };

    this.ws.onmessage = (e) => this._handleMessage(JSON.parse(e.data));

    this.ws.onclose = () => {
      this.log('WebSocket 斷線，3 秒後重連...');
      this._notifyStateChange();
      setTimeout(() => this.connect(), 3000);
    };

    this.ws.onerror = (err) => console.error('[P2P] WS error:', err);
  }

  _startStatusReporting() {
    if (this._statusInterval) clearInterval(this._statusInterval);
    this._statusInterval = setInterval(() => {
      if (!this.ws || this.ws.readyState !== WebSocket.OPEN) return;

      // 收集所有需要報告狀態的檔案 ID
      const fileIds = new Set(this.completedFiles);
      if (this.activeDownload) fileIds.add(this.activeDownload.fileId);

      // 如果沒有任何檔案也要報告基本的連線資訊（主要是 upload/download count）
      if (fileIds.size === 0) {
        this.ws.send(JSON.stringify({
          type: 'endpoint_status',
          endpoint_id: this.endpointId,
          file_id: '', // 空 ID 表示基本狀態
          owned_chunks: [],
          upload_count: this.uploadCount,
          download_count: this.webrtcDownloadCount
        }));
        return;
      }

      for (const fileId of fileIds) {
        let owned = [];
        if (this.activeDownload && this.activeDownload.fileId === fileId) {
          owned = Array.from(this.activeDownload.ownedChunks);
        } else if (this.completedFiles.has(fileId)) {
          // 如果已完成，則擁有所有區塊
          const file = this.files.find(f => f.file_id === fileId);
          if (file) {
            for (let i = 0; i < file.chunk_count; i++) owned.push(i);
          }
        }

        this.ws.send(JSON.stringify({
          type: 'endpoint_status',
          endpoint_id: this.endpointId,
          file_id: fileId,
          owned_chunks: owned,
          upload_count: this.uploadCount,
          download_count: this.webrtcDownloadCount
        }));
      }
    }, 2000);
  }

  _send(msg) {
    if (this.ws && this.ws.readyState === WebSocket.OPEN) {
      this.ws.send(JSON.stringify(msg));
    }
  }

  // ── 伺服器訊息處理 ──
  _handleMessage(msg) {
    switch (msg.type) {
      case 'registered':
        this.log(`註冊成功: ${msg.endpoint_id}`);
        this.hostEndpointId = msg.host_endpoint_id || null;
        break;
      case 'file_list':
        this.files = msg.files;
        this._notifyStateChange();
        break;
      case 'file_chunks_info':
        this._handleFileChunksInfo(msg);
        break;
      case 'suggest_peer':
        this._handleSuggestPeer(msg);
        break;
      case 'suggest_download':
        this._handleSuggestDownload(msg);
        break;
      case 'wait_and_retry':
        this._handleWaitAndRetry(msg);
        break;
      case 'webrtc_signal':
        this._handleWebRtcSignal(msg);
        break;
      case 'chunk_verify_failed_notify':
        this._handleVerifyFailedNotify(msg);
        break;
      case 'peer_disconnected':
        this._handlePeerDisconnected(msg.endpoint_id);
        break;
    }
  }

  // 來源端點斷線：立即中止所有對該 peer 的 WebRTC 下載，觸發 transfer_failed 讓中控重新分派
  _handlePeerDisconnected(peerId) {
    this.log(`收到 peer_disconnected: ${peerId.slice(0, 8)}...`);
    for (const [key, conn] of this.peerConnections) {
      // key 格式: `${peerId}-${chunkIndex}`
      if (!key.startsWith(peerId + '-')) continue;
      const { fileId, chunkIndex } = conn;
      conn.pc.close();
      this.peerConnections.delete(key);
      this.pendingRequests.delete(chunkIndex);
      this.downloadCount = Math.max(0, this.downloadCount - 1);
      this.webrtcDownloadCount = Math.max(0, this.webrtcDownloadCount - 1);
      this.log(`WebRTC 下載中止（來源端斷線）: 區塊 ${chunkIndex} ← ${peerId.slice(0, 8)}...`);
      this.uiLog(`來源端點 ${peerId.slice(0, 8)}... 已斷線，區塊 ${chunkIndex} 等待重新分派`);
      this._send({
        type: 'transfer_failed',
        endpoint_id: this.endpointId,
        file_id: fileId,
        chunk_index: chunkIndex,
        source_peer: peerId,
        reason: 'peer_disconnected'
      });
      this._send({ type: 'transfer_finished', endpoint_id: this.endpointId, file_id: fileId, chunk_index: chunkIndex, is_upload: false });
    }
    this._notifyStateChange();
  }

  _handleFileChunksInfo(msg) {
    if (!this.activeDownload || this.activeDownload.fileId !== msg.file_id) return;
    this.activeDownload.fileInfo = msg;
    this.activeDownload.totalChunks = msg.chunk_count;
    this.log(`收到檔案區塊資訊: ${msg.file_name}, ${msg.chunk_count} 個區塊`);
    // 現在只需要儲存檔案資訊，等待中控的 SuggestDownload 指令
  }

  // ── File System Access API & 啟動下載 ──
  async startDownload(fileId) {
    const file = this.files.find(f => f.file_id === fileId);
    if (!file) return;

    try {
      // 不指定 types 以避免 Windows Edge 安全警告（extensionless 檔案會被誤判為危險）
      const opts = { suggestedName: file.file_name };
      const handle = await window.showSaveFilePicker(opts);

      const downloadTask = {
        fileId,
        fileInfo: null,
        ownedChunks: new Set(),
        fileHandle: handle,
        writable: null,
        totalChunks: file.chunk_count,
        startTime: null
      };

      this.downloadQueue.push(downloadTask);
      this.log(`已加入下載排程: ${file.file_name}`);
      this.uiLog(`已加入下載佇列: ${file.file_name}`);
      this._notifyStateChange();
      this._checkQueue();
    } catch (e) {
      this.log(`下載取消或失敗: ${e.message}`);
    }
  }

  async cancelDownload(fileId) {
    // 檢查是否在排隊中
    const queueIdx = this.downloadQueue.findIndex(q => q.fileId === fileId);
    if (queueIdx !== -1) {
      const task = this.downloadQueue.splice(queueIdx, 1)[0];
      this.log(`已取消排隊：${this.files.find(f => f.file_id === fileId)?.file_name}`);
      this._notifyStateChange();
      return;
    }

    // 檢查是否正在下載
    if (this.activeDownload && this.activeDownload.fileId === fileId) {
      const dl = this.activeDownload;
      this.activeDownload = null;
      this.pendingRequests.clear();
      this.httpInFlight.clear();
      this.httpQueue = [];

      // 關閉所有 WebRTC 連線
      for (const [key, conn] of this.peerConnections) {
        if (conn.fileId === fileId) {
          conn.pc.close();
          this.peerConnections.delete(key);
        }
      }

      try {
        if (dl.writable) {
          await dl.writable.abort();
        }
      } catch (e) {
        this.log(`中止檔案寫入失敗: ${e.message}`);
      }

      const fileName = this.files.find(f => f.file_id === fileId)?.file_name;
      this.log(`已取消下載：${fileName}`);
      this._notifyStateChange();
      this._checkQueue();
    }
  }

  async _checkQueue() {
    if (this.activeDownload) return;
    if (this.downloadQueue.length === 0) return;

    const task = this.downloadQueue.shift();
    try {
      task.writable = await task.fileHandle.createWritable();
      task.startTime = Date.now();
      this.activeDownload = task;
      this.fileHandles.set(task.fileId, task.fileHandle);
      this.pendingRequests.clear();
      this.httpInFlight.clear();
      this.httpQueue = [];

      const fileName = this.files.find(f => f.file_id === task.fileId)?.file_name;
      this.log(`開始下載: ${fileName}`);
      this.uiLog(`開始下載: ${fileName}`);
      this._notifyStateChange();

      // 向中控中心回報下載意圖，會觸發 file_chunks_info 回傳
      this._send({ type: 'start_download', endpoint_id: this.endpointId, file_id: task.fileId });
    } catch (e) {
      this.log(`無法開啟寫入: ${e.message}`);
      this._checkQueue();
    }
  }

  // ── 被動等待中控分配（不再主動請求） ──
  _requestNextChunks() {
    if (!this.activeDownload) return;
    const dl = this.activeDownload;

    // 檢查是否已完成所有分片
    if (dl.ownedChunks.size >= dl.totalChunks) {
      this._finalizeDownload();
      return;
    }
    // 已改為被動等待中控的 SuggestDownload 指令
    // 此函數現在只用於檢查完成狀態
  }



  // ── 中控下載指令：主動分配下載任務 ──
  _handleSuggestDownload(msg) {
    const { file_id, chunk_index, source_peer } = msg;
    if (!this.activeDownload || this.activeDownload.fileId !== file_id) return;
    if (this.activeDownload.ownedChunks.has(chunk_index)) return; // 已擁有此分片
    if (this.pendingRequests.has(chunk_index)) return; // 已在下載中，忽略重複指派

    this.log(`中控指令: 區塊 ${chunk_index} 來自 ${source_peer.slice(0, 8)}...`);
    this.uiLog(`下載區塊 ${chunk_index}：來源端點 ${source_peer.slice(0, 8)}...`);
    this.pendingRequests.add(chunk_index);

    // 如果來源是 host（分享端），透過 HTTP 下載
    if (source_peer === this.hostEndpointId || source_peer === 'host' || !source_peer) {
      this._enqueueHttpDownload(file_id, chunk_index, source_peer);
    } else {
      this._downloadChunkViaWebRTC(file_id, chunk_index, source_peer);
    }
  }

  _httpTaskKey(fileId, chunkIndex) {
    return `${fileId}:${chunkIndex}`;
  }

  _enqueueHttpDownload(fileId, chunkIndex, sourcePeer) {
    const key = this._httpTaskKey(fileId, chunkIndex);
    if (this.httpInFlight.has(key)) return;
    if (this.httpQueue.some(t => t.fileId === fileId && t.chunkIndex === chunkIndex)) return;
    this.httpQueue.push({ fileId, chunkIndex, sourcePeer });
    this._drainHttpQueue();
  }

  _drainHttpQueue() {
    if (!this.activeDownload) return;
    // 同端點同時間只允許一條 host HTTP 下載
    if (this.httpInFlight.size > 0) return;

    while (this.httpQueue.length > 0) {
      const task = this.httpQueue.shift();
      if (!task) return;
      if (!this.activeDownload || this.activeDownload.fileId !== task.fileId) {
        this.pendingRequests.delete(task.chunkIndex);
        continue;
      }
      if (this.activeDownload.ownedChunks.has(task.chunkIndex)) {
        this.pendingRequests.delete(task.chunkIndex);
        continue;
      }
      if (!this.pendingRequests.has(task.chunkIndex)) {
        continue;
      }
      this._downloadChunkViaHttp(task.fileId, task.chunkIndex, task.sourcePeer);
      return;
    }
  }

  // ── 排程回應處理（向後兼容） ──
  _handleSuggestPeer(msg) {
    const { file_id, chunk_index, peer_id } = msg;
    this.log(`排程建議: 區塊 ${chunk_index} → 端點 ${peer_id.slice(0, 8)}...`);

    // 如果建議的是 host（分享端），透過 HTTP 下載
    if (peer_id === this.hostEndpointId || !this.hostEndpointId) {
      this._downloadChunkViaHttp(file_id, chunk_index);
    } else {
      this._downloadChunkViaWebRTC(file_id, chunk_index, peer_id);
    }
  }

  _handleWaitAndRetry(msg) {
    const { chunk_index, wait_seconds } = msg;
    this.log(`等待重試: 區塊 ${chunk_index}，${wait_seconds} 秒後`);
    this.pendingRequests.delete(chunk_index);
    // 在新模式下已不需要此邏輯，因為由中控決定何時分配
    // 但保留以向後兼容
  }

  // ── HTTP 區塊下載（從 Host 種子，屬於 HTTP pool，不計入 WebRTC 下載連線數） ──
  async _downloadChunkViaHttp(fileId, chunkIndex, sourcePeer) {
    const key = this._httpTaskKey(fileId, chunkIndex);
    if (this.httpInFlight.has(key)) return;
    this.httpInFlight.add(key);

    this.downloadCount++;
    this._notifyStateChange();
    // 注意：HTTP pool 不發送 transfer_started/finished，不佔用 WebRTC download_count
    try {
      const endpointId = encodeURIComponent(this.endpointId);
      const resp = await fetch(`/api/chunks/${fileId}/${chunkIndex}?endpoint_id=${endpointId}`);
      if (!resp.ok) throw new Error(`HTTP ${resp.status}`);
      const data = new Uint8Array(await resp.arrayBuffer());
      await this._onChunkReceived(fileId, chunkIndex, data, 'host-http');
    } catch (e) {
      this.log(`HTTP 下載區塊 ${chunkIndex} 失敗: ${e.message}`);
      this.uiLog(`區塊 ${chunkIndex} 下載失敗（HTTP），等待重試`);
      this.pendingRequests.delete(chunkIndex);
      this._send({
        type: 'transfer_failed',
        endpoint_id: this.endpointId,
        file_id: fileId,
        chunk_index: chunkIndex,
        source_peer: this.hostEndpointId || sourcePeer || 'host',
        reason: `http_exception:${e.message}`
      });
      // 中控在下次觸發時會自動重新分配此片段
    } finally {
      this.httpInFlight.delete(key);
      this.downloadCount--;
      this._notifyStateChange();
      this._drainHttpQueue();
    }
  }

  // ── WebRTC 區塊下載 ──
  async _downloadChunkViaWebRTC(fileId, chunkIndex, peerId) {
    this.downloadCount++;
    this.webrtcDownloadCount++;
    this._notifyStateChange();
    this._send({ type: 'transfer_started', endpoint_id: this.endpointId, file_id: fileId, chunk_index: chunkIndex, is_upload: false });
    this.log(`WebRTC 下載開始: file=${fileId}, chunk=${chunkIndex}, peer=${peerId.slice(0, 8)}...`);

    const key = `${peerId}-${chunkIndex}`;
    const startAt = Date.now();
    let lastProgressAt = Date.now();
    let idleTimeoutChecker = null;
    let writeChain = Promise.resolve();
    let streamEnded = false;

    try {
      const pc = new RTCPeerConnection({ iceServers: [{ urls: 'stun:stun.l.google.com:19302' }] });
      const dc = pc.createDataChannel(`chunk-${fileId}-${chunkIndex}`, { ordered: true });

      pc.onconnectionstatechange = () => {
        this.log(`WebRTC 下載連線狀態: 區塊 ${chunkIndex}，peer=${peerId.slice(0, 8)}...，state=${pc.connectionState}`);
      };
      pc.oniceconnectionstatechange = () => {
        this.log(`WebRTC 下載 ICE 狀態: 區塊 ${chunkIndex}，peer=${peerId.slice(0, 8)}...，state=${pc.iceConnectionState}`);
      };

      const chunkMeta = this.activeDownload?.fileInfo?.chunks?.[chunkIndex];
      const expectedSize = chunkMeta?.size;
      const writeBase = chunkIndex * CHUNK_SIZE;
      const receivedSlices = [];
      let receivedBytes = 0;
      let rollingCrc = CRC32.init();
      let firstSliceReceivedLogged = false;

      dc.binaryType = 'arraybuffer';
      dc.onopen = () => {
        lastProgressAt = Date.now();
        this.log(`WebRTC 下載資料通道已開啟: 區塊 ${chunkIndex} ← 來源端 ${peerId.slice(0, 8)}...`);
      };
      dc.onmessage = (e) => {
        if (typeof e.data === 'string') {
          const ctrl = JSON.parse(e.data);
          if (ctrl.type === 'chunk_complete') {
            if (streamEnded) return;
            streamEnded = true;
            lastProgressAt = Date.now();
            this.log(`WebRTC 下載收到 chunk_complete: 區塊 ${chunkIndex}，目前累計 ${receivedBytes} bytes`);
            (async () => {
              await writeChain;

              if (typeof expectedSize === 'number' && receivedBytes !== expectedSize) {
                throw new Error(`chunk size mismatch: expected ${expectedSize}, got ${receivedBytes}`);
              }

              const actualCrc = CRC32.finalize(rollingCrc);
              const chunkData = this._concatArrayBuffers(receivedSlices, receivedBytes);
              await this._onChunkReceivedStreamed(fileId, chunkIndex, actualCrc, peerId, chunkData);
              this._send({ type: 'transfer_finished', endpoint_id: this.endpointId, file_id: fileId, chunk_index: chunkIndex, is_upload: false });
            })().catch((err) => {
              this.log(`WebRTC 串流收包完成處理失敗: ${err.message}`);
              const conn = this.peerConnections.get(key);
              if (conn) conn.pc.close();
              this.peerConnections.delete(key);
              this.downloadCount = Math.max(0, this.downloadCount - 1);
              this.webrtcDownloadCount = Math.max(0, this.webrtcDownloadCount - 1);
              this.pendingRequests.delete(chunkIndex);
              this._notifyStateChange();
              this._send({
                type: 'transfer_failed',
                endpoint_id: this.endpointId,
                file_id: fileId,
                chunk_index: chunkIndex,
                source_peer: peerId,
                reason: `stream_finalize:${err.message}`
              });
              this._send({ type: 'transfer_finished', endpoint_id: this.endpointId, file_id: fileId, chunk_index: chunkIndex, is_upload: false });
            });
          }
          return;
        }

        const payload = e.data instanceof ArrayBuffer ? new Uint8Array(e.data) : new Uint8Array(e.data.buffer, e.data.byteOffset, e.data.byteLength);
        receivedSlices.push(payload.slice());
        if (!firstSliceReceivedLogged) {
          this.log(`WebRTC 下載收到首個資料切片: 區塊 ${chunkIndex}，大小 ${payload.byteLength} bytes，來源端 ${peerId.slice(0, 8)}...`);
          firstSliceReceivedLogged = true;
        }
        const position = writeBase + receivedBytes;
        receivedBytes += payload.byteLength;
        rollingCrc = CRC32.update(rollingCrc, payload);
        writeChain = writeChain.then(() => {
          if (!this.activeDownload || this.activeDownload.fileId !== fileId || !this.activeDownload.writable) {
            throw new Error('下載任務已不存在，無法寫入');
          }
          return this.activeDownload.writable.write({ type: 'write', position, data: payload });
        });
        lastProgressAt = Date.now();
      };

      dc.onclose = () => {
        if (idleTimeoutChecker) {
          clearInterval(idleTimeoutChecker);
          idleTimeoutChecker = null;
        }
        this.log(`WebRTC 下載資料通道關閉: 區塊 ${chunkIndex}，累計 ${receivedBytes} bytes`);
        this.peerConnections.delete(key);
        pc.close();

        // 若未收到 chunk_complete 就關閉，視為失敗，釋放本地/中控狀態避免卡住。
        if (!streamEnded) {
          this.pendingRequests.delete(chunkIndex);
          this.downloadCount = Math.max(0, this.downloadCount - 1);
          this.webrtcDownloadCount = Math.max(0, this.webrtcDownloadCount - 1);
          this._notifyStateChange();
          this.uiLog(`區塊 ${chunkIndex} 下載中斷（來源端 ${peerId.slice(0, 8)}... 提前關閉），等待重試`);
          this._send({
            type: 'transfer_failed',
            endpoint_id: this.endpointId,
            file_id: fileId,
            chunk_index: chunkIndex,
            source_peer: peerId,
            reason: `datachannel_closed_before_complete:recv=${receivedBytes}`
          });
          this._send({ type: 'transfer_finished', endpoint_id: this.endpointId, file_id: fileId, chunk_index: chunkIndex, is_upload: false });
        }
      };
      dc.onerror = (ev) => {
        const errMsg = ev?.error?.message || ev?.message || 'unknown';
        this.log(`WebRTC 下載資料通道錯誤: 區塊 ${chunkIndex}，peer=${peerId.slice(0, 8)}...，error=${errMsg}`);
      };

      // ICE candidate 傳送
      pc.onicecandidate = (e) => {
        if (e.candidate) {
          this._send({
            type: 'webrtc_signal', from: this.endpointId, to: peerId,
            signal: { type: 'ice', candidate: e.candidate, file_id: fileId, chunk_index: chunkIndex }
          });
        }
      };

      const offer = await pc.createOffer();
      await pc.setLocalDescription(offer);
      this._send({
        type: 'webrtc_signal', from: this.endpointId, to: peerId,
        signal: { type: 'offer', sdp: offer.sdp, file_id: fileId, chunk_index: chunkIndex }
      });
      this.log(`WebRTC 下載已送出 offer: 區塊 ${chunkIndex} -> ${peerId.slice(0, 8)}...`);

      // 存 pc 以便處理 answer
      this.peerConnections.set(key, { pc, dc, fileId, chunkIndex });

      // 無進度逾時處理：只要持續收到資料就延長，不會被固定總時限中斷。
      idleTimeoutChecker = setInterval(() => {
        if (!this.peerConnections.has(key)) {
          clearInterval(idleTimeoutChecker);
          idleTimeoutChecker = null;
          return;
        }

        if (Date.now() - lastProgressAt < WEBRTC_CHUNK_IDLE_TIMEOUT_MS) {
          return;
        }

        clearInterval(idleTimeoutChecker);
        idleTimeoutChecker = null;
        const idleForMs = Date.now() - lastProgressAt;

        const conn = this.peerConnections.get(key);
        if (conn) conn.pc.close();
        this.peerConnections.delete(key);
        this.pendingRequests.delete(chunkIndex);
        this.downloadCount = Math.max(0, this.downloadCount - 1);
        this.webrtcDownloadCount = Math.max(0, this.webrtcDownloadCount - 1);
        this._notifyStateChange();
        this.log(`WebRTC 下載區塊 ${chunkIndex} 無進度逾時（${WEBRTC_CHUNK_IDLE_TIMEOUT_MS / 1000} 秒）: 來源端 ${peerId.slice(0, 8)} 未回應（idle=${idleForMs}ms, recv=${receivedBytes} bytes, totalElapsed=${Date.now() - startAt}ms）`);
        this.uiLog(`區塊 ${chunkIndex} 下載逾時（來源端點 ${peerId.slice(0, 8)}...），等待重試`);
        this._send({
          type: 'transfer_failed',
          endpoint_id: this.endpointId,
          file_id: fileId,
          chunk_index: chunkIndex,
          source_peer: peerId,
          reason: 'idle_timeout'
        });
        this._send({ type: 'transfer_finished', endpoint_id: this.endpointId, file_id: fileId, chunk_index: chunkIndex, is_upload: false });
      }, WEBRTC_TIMEOUT_CHECK_INTERVAL_MS);
    } catch (e) {
      if (idleTimeoutChecker) {
        clearInterval(idleTimeoutChecker);
        idleTimeoutChecker = null;
      }

      const conn = this.peerConnections.get(key);
      if (conn) conn.pc.close();
      this.peerConnections.delete(key);

      this.log(`WebRTC 下載區塊 ${chunkIndex} 失敗: ${e.message}`);
      this._send({
        type: 'transfer_failed',
        endpoint_id: this.endpointId,
        file_id: fileId,
        chunk_index: chunkIndex,
        source_peer: peerId,
        reason: `exception:${e.message}`
      });
      this.downloadCount = Math.max(0, this.downloadCount - 1);
      this.webrtcDownloadCount = Math.max(0, this.webrtcDownloadCount - 1);
      this.pendingRequests.delete(chunkIndex);
      this._notifyStateChange();
      this._send({ type: 'transfer_finished', endpoint_id: this.endpointId, file_id: fileId, chunk_index: chunkIndex, is_upload: false });
      // 中控會自動重新分配失敗的分片
    }
  }

  // ── WebRTC 信令處理 ──
  async _handleWebRtcSignal(msg) {
    const { from, signal } = msg;

    if (signal.type === 'offer') {
      // 收到 offer：有人要跟我們要區塊
      this.log(`WebRTC 收到 offer: 區塊 ${signal.chunk_index} <- ${from.slice(0, 8)}...`);
      await this._handleIncomingOffer(from, signal);
    } else if (signal.type === 'answer') {
      const key = `${from}-${signal.chunk_index}`;
      const conn = this.peerConnections.get(key);
      if (conn) {
        this.log(`WebRTC 收到 answer: 區塊 ${signal.chunk_index} <- ${from.slice(0, 8)}...`);
        await conn.pc.setRemoteDescription(new RTCSessionDescription({ type: 'answer', sdp: signal.sdp }));
      } else {
        this.log(`WebRTC 收到 answer 但找不到連線: key=${key}, file=${signal.file_id || 'unknown'}`);
      }
    } else if (signal.type === 'ice') {
      const key = `${from}-${signal.chunk_index}`;
      const conn = this.peerConnections.get(key);
      if (conn) {
        await conn.pc.addIceCandidate(new RTCIceCandidate(signal.candidate));
        this.log(`WebRTC 收到 ICE: 區塊 ${signal.chunk_index} <- ${from.slice(0, 8)}...`);
      } else {
        this.log(`WebRTC 收到 ICE 但找不到連線: key=${key}, file=${signal.file_id || 'unknown'}`);
      }
    }
  }

  async _handleIncomingOffer(from, signal) {
    const { file_id, chunk_index, sdp } = signal;
    const hasChunk = this._hasChunkForUpload(file_id, chunk_index);
    if (!hasChunk) return;
    const key = `${from}-${chunk_index}`;

    this.uploadCount++;
    this._notifyStateChange();
    this._send({ type: 'transfer_started', endpoint_id: this.endpointId, file_id, chunk_index, is_upload: true });
    this.log(`上傳區塊 ${chunk_index} 給端點 ${from.slice(0, 8)}...`);

    const pc = new RTCPeerConnection({ iceServers: [{ urls: 'stun:stun.l.google.com:19302' }] });
    // 必須先註冊連線，否則對端 ICE 可能先到，會被誤判為找不到連線。
    this.peerConnections.set(key, { pc, dc: null, fileId: file_id, chunkIndex: chunk_index });
    this.log(`WebRTC 上傳連線已註冊: key=${key}`);

    pc.onicecandidate = (e) => {
      if (e.candidate) {
        this._send({
          type: 'webrtc_signal', from: this.endpointId, to: from,
          signal: { type: 'ice', candidate: e.candidate, file_id, chunk_index }
        });
      }
    };

    pc.ondatachannel = (e) => {
      const dc = e.channel;
      this.log(`WebRTC 上傳收到資料通道: 區塊 ${chunk_index} <- 端點 ${from.slice(0, 8)}...，label=${dc.label}`);
      const conn = this.peerConnections.get(key);
      if (conn) conn.dc = dc;
      let uploadSettled = false;

      const settleUpload = (options = {}) => {
        if (uploadSettled) return;
        uploadSettled = true;

        this.uploadCount = Math.max(0, this.uploadCount - 1);
        this._notifyStateChange();
        this._send({ type: 'transfer_finished', endpoint_id: this.endpointId, file_id, chunk_index, is_upload: true });

        if (options.success) {
          this.log(`上傳區塊 ${chunk_index} 完成 ✓ → 端點 ${from.slice(0, 8)}...`);
          if (!this.chunkStats.has(file_id)) this.chunkStats.set(file_id, { http: 0, webrtc: 0, upload: 0 });
          this.chunkStats.get(file_id).upload++;
        } else if (options.reason) {
          this.log(`上傳區塊 ${chunk_index} 未完成: ${options.reason} → 端點 ${from.slice(0, 8)}...`);
        }

        this.peerConnections.delete(key);
        pc.close();
      };

      dc.binaryType = 'arraybuffer';
      dc.onopen = () => {
        this.log(`WebRTC 上傳資料通道已開啟: 區塊 ${chunk_index} → 端點 ${from.slice(0, 8)}...`);
        this._sendChunkData(dc, file_id, chunk_index, from);
      };
      dc.onmessage = (ev) => {
        if (typeof ev.data === 'string') {
          const ctrl = JSON.parse(ev.data);
          if (ctrl.type === 'verify_failed') {
            this.log(`區塊 ${chunk_index} 驗證失敗通知來自 ${from.slice(0, 8)}`);
            this._reverifyChunk(file_id, chunk_index);
            settleUpload({ reason: '對端驗證失敗' });
          } else if (ctrl.type === 'chunk_received') {
            settleUpload({ success: true });
          }
        }
      };
      dc.onclose = () => {
        settleUpload({ reason: '資料通道關閉前未收到接收確認' });
      };
      dc.onerror = (ev) => {
        const errMsg = ev?.error?.message || ev?.message || 'unknown';
        this.log(`WebRTC 上傳資料通道錯誤: 區塊 ${chunk_index} -> 端點 ${from.slice(0, 8)}...，error=${errMsg}`);
        settleUpload({ reason: `資料通道錯誤: ${errMsg}` });
      };
    };

    await pc.setRemoteDescription(new RTCSessionDescription({ type: 'offer', sdp }));
    const answer = await pc.createAnswer();
    await pc.setLocalDescription(answer);
    this._send({
      type: 'webrtc_signal', from: this.endpointId, to: from,
      signal: { type: 'answer', sdp: answer.sdp, file_id, chunk_index }
    });
  }

  // ── 背壓控制發送 ──
  async _sendChunkData(dc, fileId, chunkIndex, targetPeer) {
    const chunkData = await this._readChunkFromDisk(fileId, chunkIndex);
    if (!chunkData) { dc.close(); return; }

    const SLICE_SIZE = 256 * 1024; // 256KB
    const BUFFER_THRESHOLD = 1 * 1024 * 1024; // 1MB
    let offset = 0;
    let firstSliceSentLogged = false;

    const sendSlice = () => {
      while (offset < chunkData.length) {
        if (dc.bufferedAmount > BUFFER_THRESHOLD) {
          dc.onbufferedamountlow = () => { dc.onbufferedamountlow = null; sendSlice(); };
          dc.bufferedAmountLowThreshold = BUFFER_THRESHOLD / 2;
          return;
        }
        const end = Math.min(offset + SLICE_SIZE, chunkData.length);
        if (!firstSliceSentLogged) {
          this.log(`WebRTC 上傳送出首個資料切片: 區塊 ${chunkIndex}，大小 ${end - offset} bytes → 端點 ${targetPeer.slice(0, 8)}...`);
          firstSliceSentLogged = true;
        }
        dc.send(chunkData.slice(offset, end));
        offset = end;
      }
      this.log(`WebRTC 上傳送出 chunk_complete: 區塊 ${chunkIndex}，總計 ${chunkData.length} bytes → 端點 ${targetPeer.slice(0, 8)}...`);
      dc.send(JSON.stringify({ type: 'chunk_complete', chunk_index: chunkIndex }));
    };
    sendSlice();
  }

  // ── 區塊接收與驗證 ──
  async _onChunkReceived(fileId, chunkIndex, data, sourcePeer) {
    if (!this.activeDownload || this.activeDownload.fileId !== fileId) return;
    const dl = this.activeDownload;
    const expectedCrc = dl.fileInfo?.chunks?.[chunkIndex]?.crc32;

    const actualCrc = CRC32.compute(data);
    if (expectedCrc !== undefined && actualCrc !== expectedCrc) {
      this.log(`區塊 ${chunkIndex} CRC32 驗證失敗! (期望: ${expectedCrc}, 實際: ${actualCrc})`);
      this.pendingRequests.delete(chunkIndex);

      if (sourcePeer && sourcePeer !== 'host-http') {
        // 通知來源端
        const key = `${sourcePeer}-${chunkIndex}`;
        const conn = this.peerConnections.get(key);
        if (conn && conn.dc && conn.dc.readyState === 'open') {
          conn.dc.send(JSON.stringify({ type: 'verify_failed', chunk_index: chunkIndex }));
        }
        this._send({
          type: 'chunk_verify_failed', endpoint_id: this.endpointId,
          file_id: fileId, chunk_index: chunkIndex, source_peer: sourcePeer
        });
      } else if (sourcePeer === 'host-http') {
        this._send({
          type: 'transfer_failed',
          endpoint_id: this.endpointId,
          file_id: fileId,
          chunk_index: chunkIndex,
          source_peer: this.hostEndpointId || 'host',
          reason: 'http_crc_mismatch'
        });
      }
      // 在新模式下，中控會自動重新分配，無需主動重試
      return;
    }

    // 驗證成功 → 寫入檔案 + 快取
    const position = chunkIndex * CHUNK_SIZE;
    try {
      await dl.writable.write({ type: 'write', position, data });
      // 加入快取以供上傳用
      if (!this.chunkBuffers.has(fileId)) this.chunkBuffers.set(fileId, new Map());
      this.chunkBuffers.get(fileId).set(chunkIndex, data);
    } catch (e) {
      this.log(`寫入區塊 ${chunkIndex} 失敗: ${e.message}`);
    }

    dl.ownedChunks.add(chunkIndex);
    this.pendingRequests.delete(chunkIndex);

    // 更新 HTTP / WebRTC 統計
    if (!this.chunkStats.has(fileId)) this.chunkStats.set(fileId, { http: 0, webrtc: 0, upload: 0 });
    const stats = this.chunkStats.get(fileId);
    if (sourcePeer === 'host-http') {
      stats.http++;
    } else {
      stats.webrtc++;
    }

    // 清理 peer connection
    if (sourcePeer && sourcePeer !== 'host-http') {
      const key = `${sourcePeer}-${chunkIndex}`;
      const conn = this.peerConnections.get(key);
      if (conn) { conn.pc.close(); this.peerConnections.delete(key); }
      this.downloadCount--;
      this.webrtcDownloadCount--;
    }

    this._send({
      type: 'chunk_completed', endpoint_id: this.endpointId,
      file_id: fileId, chunk_index: chunkIndex
    });

    this.log(`區塊 ${chunkIndex}/${dl.totalChunks} 完成 ✓ (${dl.ownedChunks.size}/${dl.totalChunks})`);
    this.uiLog(`區塊 ${chunkIndex} 下載完成（${dl.ownedChunks.size}/${dl.totalChunks}）`);
    this._notifyStateChange();

    // 通知中控新的完成狀態，觸發配對掃描
    // 中控會自動分配下一個任務
    this._requestNextChunks(); // 只用於檢查完成狀態
    if (sourcePeer === 'host-http') this._drainHttpQueue();
  }

  async _onChunkReceivedStreamed(fileId, chunkIndex, actualCrc, sourcePeer, chunkData = null) {
    if (!this.activeDownload || this.activeDownload.fileId !== fileId) return;
    const dl = this.activeDownload;
    const expectedCrc = dl.fileInfo?.chunks?.[chunkIndex]?.crc32;

    if (expectedCrc !== undefined && actualCrc !== expectedCrc) {
      this.log(`區塊 ${chunkIndex} CRC32 驗證失敗! (期望: ${expectedCrc}, 實際: ${actualCrc})`);
      this.pendingRequests.delete(chunkIndex);

      if (sourcePeer && sourcePeer !== 'host-http') {
        const key = `${sourcePeer}-${chunkIndex}`;
        const conn = this.peerConnections.get(key);
        if (conn && conn.dc && conn.dc.readyState === 'open') {
          conn.dc.send(JSON.stringify({ type: 'verify_failed', chunk_index: chunkIndex }));
        }
        this._send({
          type: 'chunk_verify_failed', endpoint_id: this.endpointId,
          file_id: fileId, chunk_index: chunkIndex, source_peer: sourcePeer
        });
      }
      return;
    }

    dl.ownedChunks.add(chunkIndex);
    this.pendingRequests.delete(chunkIndex);

    // 串流完成後補進快取，讓尚未 finalize 的下載端也能立即上傳該區塊。
    if (chunkData && chunkData.byteLength > 0) {
      if (!this.chunkBuffers.has(fileId)) this.chunkBuffers.set(fileId, new Map());
      this.chunkBuffers.get(fileId).set(chunkIndex, chunkData);
    }

    if (!this.chunkStats.has(fileId)) this.chunkStats.set(fileId, { http: 0, webrtc: 0, upload: 0 });
    const stats = this.chunkStats.get(fileId);
    if (sourcePeer === 'host-http') {
      stats.http++;
    } else {
      stats.webrtc++;
    }

    if (sourcePeer && sourcePeer !== 'host-http') {
      const key = `${sourcePeer}-${chunkIndex}`;
      const conn = this.peerConnections.get(key);
      if (conn) {
        if (conn.dc && conn.dc.readyState === 'open') {
          this.log(`WebRTC 下載送出接收確認: 區塊 ${chunkIndex} → 來源端 ${sourcePeer.slice(0, 8)}...`);
          conn.dc.send(JSON.stringify({ type: 'chunk_received', chunk_index: chunkIndex }));
        }
        this.log(`WebRTC 下載關閉連線: 區塊 ${chunkIndex}，來源端 ${sourcePeer.slice(0, 8)}...`);
        conn.pc.close();
        this.peerConnections.delete(key);
      }
      this.downloadCount = Math.max(0, this.downloadCount - 1);
      this.webrtcDownloadCount = Math.max(0, this.webrtcDownloadCount - 1);
    }

    this._send({
      type: 'chunk_completed', endpoint_id: this.endpointId,
      file_id: fileId, chunk_index: chunkIndex
    });

    this.log(`區塊 ${chunkIndex}/${dl.totalChunks} 完成 ✓ (${dl.ownedChunks.size}/${dl.totalChunks})`);
    this.uiLog(`區塊 ${chunkIndex} 下載完成（${dl.ownedChunks.size}/${dl.totalChunks}）`);
    this._notifyStateChange();
    this._requestNextChunks();
  }

  async _finalizeDownload() {
    if (!this.activeDownload) return;
    const dl = this.activeDownload;
    try {
      await dl.writable.close();
    } catch (e) {
      this.log(`關閉檔案失敗: ${e.message}`);
    }
    const elapsed = ((Date.now() - dl.startTime) / 1000).toFixed(1);
    this.log(`下載完成! 耗時 ${elapsed} 秒`);
    this.uiLog(`檔案下載完成，耗時 ${elapsed} 秒`);
    this.completedFiles.add(dl.fileId);
    // 下載完成後清除該檔案的快取，轉為從磁碟讀取
    this.chunkBuffers.delete(dl.fileId);
    this.activeDownload = null;
    this._notifyStateChange();
    this._checkQueue();
  }

  async _reverifyChunk(fileId, chunkIndex) {
    const buf = await this._readChunkFromDisk(fileId, chunkIndex);
    if (!buf) return;
    const expected = this.activeDownload?.fileInfo?.chunks?.[chunkIndex]?.crc32;
    if (expected !== undefined && CRC32.compute(buf) !== expected) {
      this.log(`自我驗證區塊 ${chunkIndex} 失敗，捨棄`);
      if (this.activeDownload) this.activeDownload.ownedChunks.delete(chunkIndex);
    }
  }

  _hasChunkForUpload(fileId, chunkIndex) {
    if (this.activeDownload && this.activeDownload.fileId === fileId && this.activeDownload.ownedChunks.has(chunkIndex)) {
      return true;
    }
    if (this.completedFiles.has(fileId)) return true;
    return false;
  }

  async _readChunkFromDisk(fileId, chunkIndex) {
    // 1. 優先從快取讀取（下載中的檔案）
    if (this.chunkBuffers.has(fileId)) {
      const cached = this.chunkBuffers.get(fileId).get(chunkIndex);
      if (cached) {
        this.log(`磁碟讀取: 區塊 ${chunkIndex} 從快取中讀取，大小 ${cached.byteLength} bytes`);
        return cached;
      }
    }

    // 2. 落回磁碟讀取（已完成的檔案）
    const handle = this.fileHandles.get(fileId)
      || (this.activeDownload && this.activeDownload.fileId === fileId ? this.activeDownload.fileHandle : null);
    if (!handle) {
      this.log(`磁碟讀取前: 找不到檔案 handle，無法讀取區塊 ${chunkIndex} (fileId=${fileId})`);
      return null;
    }

    try {
      this.log(`磁碟讀取前: 準備讀取區塊 ${chunkIndex} (fileId=${fileId})`);
      const file = await handle.getFile();
      this.log(`磁碟讀取 getFile 完成: 區塊 ${chunkIndex}，檔案大小 ${file.size} bytes`);
      const start = chunkIndex * CHUNK_SIZE;
      if (start >= file.size) return null;
      const end = Math.min(start + CHUNK_SIZE, file.size);
      this.log(`磁碟讀取 arrayBuffer 前: 區塊 ${chunkIndex}，範圍 ${start}-${end}`);
      const data = new Uint8Array(await file.slice(start, end).arrayBuffer());
      this.log(`磁碟讀取 arrayBuffer 完成: 區塊 ${chunkIndex}，讀取 ${data.byteLength} bytes`);
      return data;
    } catch (e) {
      this.log(`從磁碟讀取區塊 ${chunkIndex} 失敗: ${e.message}`);
      return null;
    }
  }

  _concatArrayBuffers(arrays, totalLength) {
    const result = new Uint8Array(totalLength);
    let offset = 0;
    for (const arr of arrays) { result.set(arr, offset); offset += arr.length; }
    return result;
  }

  _notifyStateChange() {
    if (this.onStateChange) this.onStateChange();
  }

  get isConnected() {
    return this.ws && this.ws.readyState === WebSocket.OPEN;
  }

  get progress() {
    if (!this.activeDownload) return null;
    const dl = this.activeDownload;
    return {
      owned: dl.ownedChunks.size,
      total: dl.totalChunks,
      percent: dl.totalChunks > 0 ? Math.round((dl.ownedChunks.size / dl.totalChunks) * 100) : 0
    };
  }

  get queuedFiles() {
    return this.downloadQueue.map(q => q.fileId);
  }
}
