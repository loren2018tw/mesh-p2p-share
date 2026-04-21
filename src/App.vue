<script setup lang="ts">
import { ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";

const selectedFile = ref<string | null>(null);
const message = ref("");
const error = ref("");

async function selectFile() {
  try {
    error.value = "";
    message.value = "";
    const file = await open({
      multiple: false,
      directory: false,
    });
    
    if (file) {
      selectedFile.value = file.path;
      await invoke("share_file", { path: file.path });
      message.value = "檔案已設定為分享狀態";
    }
  } catch (e: any) {
    error.value = `發生錯誤: ${e}`;
  }
}
</script>

<template>
  <v-app>
    <v-app-bar title="Mesh P2P Share" color="primary"></v-app-bar>
    <v-main>
      <v-container class="fill-height">
        <v-row justify="center" align="center">
          <v-col cols="12" md="8" lg="6">
            <v-card class="pa-6 text-center" elevation="4">
              <v-icon icon="mdi-file-upload-outline" size="64" color="primary" class="mb-4"></v-icon>
              <h2 class="text-h5 mb-4">分享檔案給其他人</h2>
              <p class="text-body-1 mb-6 text-medium-emphasis">
                點擊下方按鈕選擇您想要分享的檔案。當選擇完成後，我們將為您建立一個下載連結。
              </p>
              
              <v-btn
                color="primary"
                size="x-large"
                prepend-icon="mdi-folder-open"
                @click="selectFile"
              >
                選擇檔案
              </v-btn>

              <v-expand-transition>
                <div v-if="selectedFile" class="mt-6">
                  <v-alert
                    type="success"
                    variant="tonal"
                    :text="message"
                    class="mb-4"
                  ></v-alert>
                  <v-text-field
                    v-model="selectedFile"
                    label="已選取的檔案路徑"
                    readonly
                    variant="outlined"
                    prepend-inner-icon="mdi-file"
                  ></v-text-field>
                </div>
              </v-expand-transition>

              <v-alert
                v-if="error"
                type="error"
                variant="tonal"
                class="mt-4"
                :text="error"
              ></v-alert>
            </v-card>
          </v-col>
        </v-row>
      </v-container>
    </v-main>
  </v-app>
</template>

<style>
html, body {
  overflow-y: hidden;
}
</style>