<template>
  <button @click="$emit('select', 'copyAccessToken')" class="dropdown-item">
    <svg width="14" height="14" viewBox="0 0 24 24" fill="currentColor">
      <path d="M16 1H4c-1.1 0-2 .9-2 2v14h2V3h12V1zm3 4H8c-1.1 0-2 .9-2 2v14c0 1.1.9 2 2 2h11c1.1 0 2-.9 2-2V7c0-1.1-.9-2-2-2zm0 16H8V7h11v14z"/>
    </svg>
    <span>{{ $t('accountCard.copyAccessToken') }}</span>
  </button>
  <button
    v-if="hasSessionToken"
    @click="$emit('select', 'copySessionToken')"
    class="dropdown-item"
  >
    <svg width="14" height="14" viewBox="0 0 24 24" fill="currentColor">
      <path d="M16 1H4c-1.1 0-2 .9-2 2v14h2V3h12V1zm3 4H8c-1.1 0-2 .9-2 2v14c0 1.1.9 2 2 2h11c1.1 0 2-.9 2-2V7c0-1.1-.9-2-2-2zm0 16H8V7h11v14z"/>
    </svg>
    <span>{{ $t('accountCard.copySessionToken') }}</span>
  </button>
  <button
    v-if="hasSessionToken"
    @click="$emit('select', 'activeSessions')"
    class="dropdown-item"
    v-tooltip="$t('cursorSessions.entryHint')"
  >
    <svg width="14" height="14" viewBox="0 0 24 24" fill="currentColor">
      <path d="M4 6h16v10H4V6zm0-2c-1.1 0-2 .9-2 2v10c0 1.1.9 2 2 2h5v2h6v-2h5c1.1 0 2-.9 2-2V6c0-1.1-.9-2-2-2H4z"/>
    </svg>
    <span>{{ $t('cursorSessions.entry') }}</span>
  </button>
  <button
    @click="$emit('select', 'generateMachineId')"
    class="dropdown-item"
    :disabled="isGeneratingMachineId"
  >
    <span v-if="isGeneratingMachineId" class="btn-spinner btn-spinner--xs"></span>
    <svg v-else width="14" height="14" viewBox="0 0 24 24" fill="currentColor">
      <path d="M19 13h-6v6h-2v-6H5v-2h6V5h2v6h6v2z"/>
    </svg>
    <span>{{ $t('accountCard.generateAndBindMachineId') }}</span>
  </button>
  <button @click="$emit('select', 'export')" class="dropdown-item">
    <svg width="14" height="14" viewBox="0 0 24 24" fill="currentColor">
      <path d="M19 9h-4V3H9v6H5l7 7 7-7zM5 18v2h14v-2H5z"/>
    </svg>
    <span>{{ $t('accountCard.export') }}</span>
  </button>
  <button @click="$emit('select', 'delete')" class="dropdown-item text-danger hover:bg-danger/10">
    <svg width="14" height="14" viewBox="0 0 24 24" fill="currentColor">
      <path d="M6 19c0 1.1.9 2 2 2h8c1.1 0 2-.9 2-2V7H6v12zM19 4h-3.5l-1-1h-5l-1 1H5v2h14V4z"/>
    </svg>
    <span>{{ $t('common.delete') }}</span>
  </button>
</template>

<script setup>
/**
 * Cursor 账号操作菜单项，卡片与表格行共用。
 * 只负责渲染与上报选中的动作类型，具体行为由 useCursorAccount.handleMenuClick 分发。
 */
defineProps({
  hasSessionToken: { type: Boolean, default: false },
  isGeneratingMachineId: { type: Boolean, default: false }
})

defineEmits(['select'])
</script>
