<script setup lang="ts">
import { onMounted, ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import QRCode from "qrcode";

interface SharedFile {
  path: string;
  name: string;
  size: string;
  file_id: string;
  chunk_count: number;
  processing: boolean;
}

interface FileListItem {
  file_id: string;
  file_name: string;
  total_size: number;
  chunk_count: number;
}

const serviceUrl = ref("");
const qrCodeDataUrl = ref("");
const qrDialog = ref(false);
const appVersion = ref("...");
const snackbar = ref(false);
const snackbarText = ref("");
const snackbarColor = ref("success");
const sharedFiles = ref<SharedFile[]>([]);

async function loadServiceUrl(retry = 20) {
  try {
    const url = await invoke<string>("get_service_url");
    serviceUrl.value = url;
    qrCodeDataUrl.value = await QRCode.toDataURL(url, {
      width: 800, // 高解析度以便放大不失真
      margin: 2,
      color: { dark: "#5C3D1E", light: "#F8F2E4" },
    });
  } catch {
    if (retry > 0) {
      setTimeout(() => loadServiceUrl(retry - 1), 1000);
    }
  }
}

function formatBytes(bytes: number): string {
  if (bytes === 0) return "0 B";
  const sizes = ["B", "KB", "MB", "GB", "TB"];
  const i = Math.floor(Math.log(bytes) / Math.log(1024));
  return `${(bytes / 1024 ** i).toFixed(1)} ${sizes[i]}`;
}

function notify(text: string, color = "success") {
  snackbarText.value = text;
  snackbarColor.value = color;
  snackbar.value = true;
}

async function copyServiceUrl() {
  if (!serviceUrl.value) {
    notify("服務網址尚未準備好，請稍後再試。", "warning");
    return;
  }
  try {
    await navigator.clipboard.writeText(serviceUrl.value);
    notify("已複製入口網址至剪貼簿");
  } catch (e: any) {
    notify(`無法複製：${e}`, "error");
  }
}

async function addSharedFile(path: string) {
  const exists = sharedFiles.value.some((item) => item.path === path);
  if (exists) {
    notify("此檔案已在分享清單中", "warning");
    return;
  }
  // 先加入清單（顯示處理中狀態）
  const name = path.split(/[\\/]/).pop() || path;
  const tempEntry: SharedFile = {
    path,
    name,
    size: "處理中...",
    file_id: "",
    chunk_count: 0,
    processing: true,
  };
  sharedFiles.value.push(tempEntry);

  try {
    const result = await invoke<FileListItem>("share_file", { path });
    // 更新已處理完成的資訊
    const idx = sharedFiles.value.findIndex((f) => f.path === path);
    if (idx >= 0) {
      sharedFiles.value[idx] = {
        path,
        name: result.file_name,
        size: formatBytes(result.total_size),
        file_id: result.file_id,
        chunk_count: result.chunk_count,
        processing: false,
      };
    }
    notify(`檔案已新增至分享清單（${result.chunk_count} 個區塊）`);
  } catch (e: any) {
    // 處理失敗時移除
    sharedFiles.value = sharedFiles.value.filter((f) => f.path !== path);
    notify(`檔案處理失敗：${e}`, "error");
  }
}

async function addFile() {
  try {
    const file = await open({ multiple: false, directory: false });
    const path =
      typeof file === "string"
        ? file
        : file && typeof file === "object" && "path" in file
          ? (file as any).path
          : null;
    if (path) await addSharedFile(path);
  } catch (e: any) {
    notify(`發生錯誤：${e}`, "error");
  }
}

async function removeFile(path: string) {
  try {
    await invoke("remove_shared_file", { path });
  } catch {}
  sharedFiles.value = sharedFiles.value.filter((f) => f.path !== path);
  notify("已移除分享檔案", "info");
}

onMounted(async () => {
  loadServiceUrl();
  try {
    appVersion.value = await invoke<string>("get_app_version");
  } catch {}
});
</script>

<template>
  <v-app>
    <!-- App Bar -->
    <v-app-bar flat color="primary" elevation="2">
      <template #prepend>
        <v-icon icon="mdi-share-circle" class="ml-3" color="white" />
      </template>
      <v-app-bar-title class="app-title text-white font-weight-bold">
        Mesh P2P Share
      </v-app-bar-title>
      <template #append>
        <div class="d-flex align-center text-caption text-white mr-4">
          <span class="mr-2">v{{ appVersion }} by Loren(loren.tw@gmail.com)</span>
          <v-btn
            icon="mdi-github"
            variant="text"
            size="small"
            color="white"
            href="https://github.com/loren2018tw/mesh-p2p-share"
            target="_blank"
          />
        </div>
      </template>
    </v-app-bar>

    <v-main>
      <v-container class="py-8 px-4">
        <v-row justify="center">
          <v-col cols="12" lg="9" xl="7">
            <!-- ── 入口網址 Card ── -->
            <v-card class="mb-5 url-card" elevation="2" rounded="lg">
              <v-card-item class="pt-5 pb-2">
                <template #prepend>
                  <v-avatar color="primary" variant="tonal" size="44">
                    <v-icon icon="mdi-web" />
                  </v-avatar>
                </template>
                <v-card-title class="text-h6 font-weight-bold section-title">
                  下載端入口網址
                </v-card-title>
                <v-card-subtitle class="mt-1">
                  讓下載端的瀏覽器連至此網址以瀏覽並下載分享的檔案
                </v-card-subtitle>
              </v-card-item>

              <v-divider class="mx-4" />

              <v-card-text class="pt-4 pb-5">
                <v-row align="center" no-gutters>
                  <!-- URL 純文字顯示 -->
                  <v-col>
                    <div v-if="serviceUrl" class="url-display pa-3">
                      <span class="url-text">{{ serviceUrl }}</span>
                    </div>
                    <div v-else class="url-loading pa-3 d-flex align-center">
                      <v-progress-circular
                        indeterminate
                        color="primary"
                        size="18"
                        width="2"
                        class="mr-3"
                      />
                      <span class="text-medium-emphasis"
                        >服務啟動中，請稍候…</span
                      >
                    </div>

                    <v-btn
                      color="primary"
                      variant="tonal"
                      rounded="pill"
                      size="small"
                      prepend-icon="mdi-content-copy"
                      class="mt-3"
                      :disabled="!serviceUrl"
                      @click="copyServiceUrl"
                    >
                      複製連結
                    </v-btn>
                  </v-col>

                  <!-- QR Code -->
                  <v-col cols="auto" class="ml-6">
                    <v-tooltip text="點擊放大 QR Code" location="top">
                      <template #activator="{ props }">
                        <div
                          v-bind="props"
                          class="qr-wrapper"
                          :class="{ 'qr-ready': !!qrCodeDataUrl }"
                          @click="qrDialog = true"
                        >
                          <img
                            v-if="qrCodeDataUrl"
                            :src="qrCodeDataUrl"
                            alt="QR Code"
                            width="108"
                            height="108"
                          />
                          <div v-else class="qr-placeholder">
                            <v-icon
                              icon="mdi-qrcode"
                              size="40"
                              color="primary"
                              class="opacity-30"
                            />
                          </div>
                        </div>
                      </template>
                    </v-tooltip>
                  </v-col>
                </v-row>
              </v-card-text>
            </v-card>

            <!-- ── 分享檔案清單 Card ── -->
            <v-card elevation="2" rounded="lg">
              <v-card-item class="pt-5 pb-2">
                <template #prepend>
                  <v-avatar color="secondary" variant="tonal" size="44">
                    <v-icon icon="mdi-file-multiple-outline" />
                  </v-avatar>
                </template>
                <v-card-title class="text-h6 font-weight-bold section-title">
                  分享檔案清單
                </v-card-title>
                <v-card-subtitle class="mt-1">
                  {{
                    sharedFiles.length > 0
                      ? `共 ${sharedFiles.length} 個檔案`
                      : "尚未加入任何檔案"
                  }}
                </v-card-subtitle>
                <template #append>
                  <v-btn
                    color="secondary"
                    variant="tonal"
                    rounded="pill"
                    prepend-icon="mdi-plus"
                    @click="addFile"
                  >
                    新增檔案
                  </v-btn>
                </template>
              </v-card-item>

              <v-divider class="mx-4" />

              <v-card-text class="pa-0">
                <v-list
                  v-if="sharedFiles.length"
                  lines="two"
                  bg-color="transparent"
                  class="py-0"
                >
                  <template v-for="(file, idx) in sharedFiles" :key="file.path">
                    <v-list-item
                      :title="file.name"
                      :subtitle="file.processing ? '處理中...' : `${file.size} · ${file.chunk_count} 個區塊`"
                      class="py-3"
                    >
                      <template #prepend>
                        <v-avatar
                          color="primary"
                          variant="tonal"
                          size="40"
                          class="mr-1"
                        >
                          <v-icon icon="mdi-file-outline" size="20" />
                        </v-avatar>
                      </template>
                      <template #append>
                        <v-btn
                          icon="mdi-delete-outline"
                          color="error"
                          variant="text"
                          size="small"
                          @click="removeFile(file.path)"
                        />
                      </template>
                    </v-list-item>
                    <v-divider v-if="idx < sharedFiles.length - 1" inset />
                  </template>
                </v-list>

                <div v-else class="empty-state">
                  <v-icon
                    icon="mdi-tray-arrow-up"
                    size="52"
                    color="primary"
                    class="mb-3 opacity-20"
                  />
                  <div class="text-body-1 font-weight-medium">
                    尚未加入任何檔案
                  </div>
                  <div class="text-caption text-medium-emphasis mt-1">
                    點擊「新增檔案」開始分享
                  </div>
                </div>
              </v-card-text>
            </v-card>
          </v-col>
        </v-row>
      </v-container>
    </v-main>

    <!-- QR 放大 Dialog -->
    <v-dialog v-model="qrDialog" width="auto">
      <v-card rounded="lg" class="text-center" style="max-width: 95vw; max-height: 95vh; display: flex; flex-direction: column;">
        <v-card-title class="pt-6 text-h6 font-weight-bold section-title flex-shrink-0"
          >掃描 QR Code</v-card-title
        >
        <v-card-subtitle class="mb-2 flex-shrink-0">下載端掃描後即可連入</v-card-subtitle>
        <v-card-text class="pb-5 d-flex justify-center align-center" style="overflow: hidden;">
          <img
            v-if="qrCodeDataUrl"
            :src="qrCodeDataUrl"
            alt="QR Code"
            class="qr-dialog-img"
            style="max-width: 100%; max-height: 65vh; object-fit: contain; border-radius: 8px;"
          />
        </v-card-text>
        <v-card-actions class="pb-4">
          <v-spacer />
          <v-btn
            color="primary"
            variant="text"
            rounded="pill"
            @click="qrDialog = false"
            >關閉</v-btn
          >
        </v-card-actions>
      </v-card>
    </v-dialog>

    <!-- Snackbar 通知 -->
    <v-snackbar
      v-model="snackbar"
      :color="snackbarColor"
      timeout="2500"
      location="bottom"
      rounded="pill"
    >
      {{ snackbarText }}
      <template #actions>
        <v-btn
          icon="mdi-close"
          variant="text"
          size="small"
          @click="snackbar = false"
        />
      </template>
    </v-snackbar>
  </v-app>
</template>

<style>
html,
body {
  overflow-y: hidden;
}

/* App bar title letter-spacing */
.app-title {
  letter-spacing: 0.06em;
}

/* Card section titles */
.section-title {
  letter-spacing: 0.04em;
}

/* URL field monospace */
.url-field input {
  font-family: "Courier New", monospace;
  font-size: 0.88rem;
  letter-spacing: 0.02em;
}

/* URL 純文字顯示框 */
.url-display {
  background: rgba(123, 85, 53, 0.07);
  border: 1.5px solid rgba(123, 85, 53, 0.25);
  border-radius: 8px;
  min-height: 44px;
  display: flex;
  align-items: center;
}
.url-text {
  font-family: "Courier New", monospace;
  font-size: 0.9rem;
  letter-spacing: 0.03em;
  color: #5c3d1e;
  word-break: break-all;
}
.url-loading {
  background: rgba(123, 85, 53, 0.04);
  border: 1.5px dashed rgba(123, 85, 53, 0.2);
  border-radius: 8px;
  min-height: 44px;
}

/* Copy icon hover */
.copy-icon {
  cursor: pointer;
  opacity: 0.7;
  transition: opacity 0.15s;
}
.copy-icon:hover {
  opacity: 1;
}

/* QR Code wrapper */
.qr-wrapper {
  width: 108px;
  height: 108px;
  border-radius: 10px;
  overflow: hidden;
  border: 2px solid rgba(123, 85, 53, 0.2);
  background: #f8f2e4;
  display: flex;
  align-items: center;
  justify-content: center;
  transition:
    border-color 0.2s,
    transform 0.15s;
}
.qr-wrapper.qr-ready {
  cursor: pointer;
}
.qr-wrapper.qr-ready:hover {
  border-color: rgba(123, 85, 53, 0.55);
  transform: scale(1.03);
}

/* QR dialog image border */
.qr-dialog-img {
  border-radius: 8px;
  border: 2px solid rgba(123, 85, 53, 0.15);
}

/* Empty state */
.empty-state {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  padding: 48px 24px;
}
</style>
