<script setup lang="ts">
import { useNotification } from "naive-ui"
import { setNotificationApi } from "@/utils/notify"

// App.vue 的 useTabs 等 setup 外上下文无法直接 useNotification，
// 由本组件（挂在 NNotificationProvider 下）注入 API 供全局使用
setNotificationApi(useNotification())
</script>

<template>
  <div hidden></div>
</template>
