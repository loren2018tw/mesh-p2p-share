// ── CRC32 計算工具 ──
const CRC32 = (() => {
  const table = new Uint32Array(256);
  for (let i = 0; i < 256; i++) {
    let c = i;
    for (let j = 0; j < 8; j++) c = c & 1 ? 0xEDB88320 ^ (c >>> 1) : c >>> 1;
    table[i] = c;
  }
  return {
    compute(data) {
      let crc = 0xFFFFFFFF;
      const u8 = data instanceof Uint8Array ? data : new Uint8Array(data);
      for (let i = 0; i < u8.length; i++) crc = table[(crc ^ u8[i]) & 0xFF] ^ (crc >>> 8);
      return (crc ^ 0xFFFFFFFF) >>> 0;
    }
  };
})();

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
    this.chunkBuffers = new Map(); // fileId -> Map<chunkIndex, Uint8Array>
    this.pendingRequests = new Set(); // chunk indices currently being requested/downloaded
    this.onStateChange = null;
    this.onLog = null;
    this.hostEndpointId = null;
    this.maxConcurrentDownloads = 3;
    this.completedFiles = new Set(); // 已完成下載的 fileId
    this.downloadQueue = []; // { fileId, fileHandle, totalChunks, ... }
  }

  log(msg) {
    console.log(`[P2P] ${msg}`);
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
          download_count: this.downloadCount
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
          download_count: this.downloadCount
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
    }
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
      this.pendingRequests.clear();

      const fileName = this.files.find(f => f.file_id === task.fileId)?.file_name;
      this.log(`開始下載: ${fileName}`);
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

    this.log(`中控指令: 區塊 ${chunk_index} 來自 ${source_peer.slice(0, 8)}...`);
    this.pendingRequests.add(chunk_index);

    // 如果來源是 host（分享端），透過 HTTP 下載
    if (source_peer === this.hostEndpointId || source_peer === 'host' || !source_peer) {
      this._downloadChunkViaHttp(file_id, chunk_index);
    } else {
      this._downloadChunkViaWebRTC(file_id, chunk_index, source_peer);
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

  // ── HTTP 區塊下載（從 Host 種子） ──
  async _downloadChunkViaHttp(fileId, chunkIndex) {
    this.downloadCount++;
    this._notifyStateChange();
    this._send({ type: 'transfer_started', endpoint_id: this.endpointId, file_id: fileId, chunk_index: chunkIndex, is_upload: false });
    try {
      const resp = await fetch(`/api/chunks/${fileId}/${chunkIndex}`);
      if (!resp.ok) throw new Error(`HTTP ${resp.status}`);
      const data = new Uint8Array(await resp.arrayBuffer());
      await this._onChunkReceived(fileId, chunkIndex, data, 'host-http');
    } catch (e) {
      this.log(`HTTP 下載區塊 ${chunkIndex} 失敗: ${e.message}`);
      this.pendingRequests.delete(chunkIndex);
      // 在新模式下，中控會自動重新分配失敗的分片
    } finally {
      this.downloadCount--;
      this._notifyStateChange();
      this._send({ type: 'transfer_finished', endpoint_id: this.endpointId, file_id: fileId, chunk_index: chunkIndex, is_upload: false });
    }
  }

  // ── WebRTC 區塊下載 ──
  async _downloadChunkViaWebRTC(fileId, chunkIndex, peerId) {
    this.downloadCount++;
    this._notifyStateChange();
    this._send({ type: 'transfer_started', endpoint_id: this.endpointId, file_id: fileId, chunk_index: chunkIndex, is_upload: false });
    try {
      const pc = new RTCPeerConnection({ iceServers: [{ urls: 'stun:stun.l.google.com:19302' }] });
      const dc = pc.createDataChannel(`chunk-${fileId}-${chunkIndex}`, { ordered: true });

      const received = [];
      let totalReceived = 0;

      dc.binaryType = 'arraybuffer';
      dc.onmessage = (e) => {
        if (typeof e.data === 'string') {
          const ctrl = JSON.parse(e.data);
          if (ctrl.type === 'chunk_complete') {
            const fullData = this._concatArrayBuffers(received, totalReceived);
            this._onChunkReceived(fileId, chunkIndex, fullData, peerId);
            this._send({ type: 'transfer_finished', endpoint_id: this.endpointId, file_id: fileId, chunk_index: chunkIndex, is_upload: false });
          }
          return;
        }
        received.push(new Uint8Array(e.data));
        totalReceived += e.data.byteLength;
      };

      dc.onclose = () => pc.close();

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

      // 存 pc 以便處理 answer
      this.peerConnections.set(`${peerId}-${chunkIndex}`, { pc, dc, fileId, chunkIndex });

      // 超時處理
      setTimeout(() => {
        const key = `${peerId}-${chunkIndex}`;
        if (this.peerConnections.has(key)) {
          this.peerConnections.get(key).pc.close();
          this.peerConnections.delete(key);
          this.pendingRequests.delete(chunkIndex);
          this.downloadCount--;
          this._notifyStateChange();
          this._send({ type: 'transfer_finished', endpoint_id: this.endpointId, file_id: fileId, chunk_index: chunkIndex, is_upload: false });
        }
      }, 30000);
    } catch (e) {
      this.log(`WebRTC 下載區塊 ${chunkIndex} 失敗: ${e.message}`);
      this.downloadCount--;
      this.pendingRequests.delete(chunkIndex);
      this._notifyStateChange();
      // 中控會自動重新分配失敗的分片
    }
  }

  // ── WebRTC 信令處理 ──
  async _handleWebRtcSignal(msg) {
    const { from, signal } = msg;

    if (signal.type === 'offer') {
      // 收到 offer：有人要跟我們要區塊
      await this._handleIncomingOffer(from, signal);
    } else if (signal.type === 'answer') {
      const key = `${from}-${signal.chunk_index}`;
      const conn = this.peerConnections.get(key);
      if (conn) {
        await conn.pc.setRemoteDescription(new RTCSessionDescription({ type: 'answer', sdp: signal.sdp }));
      }
    } else if (signal.type === 'ice') {
      const key = `${from}-${signal.chunk_index}`;
      const conn = this.peerConnections.get(key);
      if (conn) {
        await conn.pc.addIceCandidate(new RTCIceCandidate(signal.candidate));
      }
    }
  }

  async _handleIncomingOffer(from, signal) {
    const { file_id, chunk_index, sdp } = signal;
    if (!this.activeDownload || !this.activeDownload.ownedChunks.has(chunk_index)) return;

    this.uploadCount++;
    this._notifyStateChange();
    this._send({ type: 'transfer_started', endpoint_id: this.endpointId, file_id, chunk_index, is_upload: true });

    const pc = new RTCPeerConnection({ iceServers: [{ urls: 'stun:stun.l.google.com:19302' }] });

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
      dc.binaryType = 'arraybuffer';
      dc.onopen = () => {
        this._sendChunkData(dc, file_id, chunk_index, from);
      };
      dc.onmessage = (ev) => {
        if (typeof ev.data === 'string') {
          const ctrl = JSON.parse(ev.data);
          if (ctrl.type === 'verify_failed') {
            this.log(`區塊 ${chunk_index} 驗證失敗通知來自 ${from.slice(0, 8)}`);
            this._reverifyChunk(file_id, chunk_index);
          }
        }
      };
      dc.onclose = () => {
        this.uploadCount--;
        this._notifyStateChange();
        this._send({ type: 'transfer_finished', endpoint_id: this.endpointId, file_id, chunk_index, is_upload: true });
        pc.close();
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
    const chunkData = this.chunkBuffers.get(fileId)?.get(chunkIndex);
    if (!chunkData) { dc.close(); return; }

    const SLICE_SIZE = 256 * 1024; // 256KB
    const BUFFER_THRESHOLD = 1 * 1024 * 1024; // 1MB
    let offset = 0;

    const sendSlice = () => {
      while (offset < chunkData.length) {
        if (dc.bufferedAmount > BUFFER_THRESHOLD) {
          dc.onbufferedamountlow = () => { dc.onbufferedamountlow = null; sendSlice(); };
          dc.bufferedAmountLowThreshold = BUFFER_THRESHOLD / 2;
          return;
        }
        const end = Math.min(offset + SLICE_SIZE, chunkData.length);
        dc.send(chunkData.slice(offset, end));
        offset = end;
      }
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
      }
      // 在新模式下，中控會自動重新分配，無需主動重試
      return;
    }

    // 驗證成功 → 寫入檔案
    const CHUNK_SIZE = 50 * 1024 * 1024;
    const position = chunkIndex * CHUNK_SIZE;
    try {
      await dl.writable.write({ type: 'write', position, data });
    } catch (e) {
      this.log(`寫入區塊 ${chunkIndex} 失敗: ${e.message}`);
    }

    // 儲存到 buffer (供其他端點取用)
    if (!this.chunkBuffers.has(fileId)) this.chunkBuffers.set(fileId, new Map());
    this.chunkBuffers.get(fileId).set(chunkIndex, data);

    dl.ownedChunks.add(chunkIndex);
    this.pendingRequests.delete(chunkIndex);

    // 清理 peer connection
    if (sourcePeer && sourcePeer !== 'host-http') {
      const key = `${sourcePeer}-${chunkIndex}`;
      const conn = this.peerConnections.get(key);
      if (conn) { conn.pc.close(); this.peerConnections.delete(key); }
      this.downloadCount--;
    }

    this._send({
      type: 'chunk_completed', endpoint_id: this.endpointId,
      file_id: fileId, chunk_index: chunkIndex
    });

    this.log(`區塊 ${chunkIndex}/${dl.totalChunks} 完成 ✓ (${dl.ownedChunks.size}/${dl.totalChunks})`);
    this._notifyStateChange();

    // 通知中控新的完成狀態，觸發配對掃描
    // 中控會自動分配下一個任務
    this._requestNextChunks(); // 只用於檢查完成狀態
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
    this.completedFiles.add(dl.fileId);
    this.activeDownload = null;
    this._notifyStateChange();
    this._checkQueue();
  }

  _reverifyChunk(fileId, chunkIndex) {
    const buf = this.chunkBuffers.get(fileId)?.get(chunkIndex);
    if (!buf) return;
    const expected = this.activeDownload?.fileInfo?.chunks?.[chunkIndex]?.crc32;
    if (expected !== undefined && CRC32.compute(buf) !== expected) {
      this.log(`自我驗證區塊 ${chunkIndex} 失敗，捨棄`);
      this.chunkBuffers.get(fileId).delete(chunkIndex);
      if (this.activeDownload) this.activeDownload.ownedChunks.delete(chunkIndex);
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
