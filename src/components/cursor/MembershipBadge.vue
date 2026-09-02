<template>
  <span :class="badgeClass">
    <svg v-if="tier === 'ultra'" class="w-3 h-3" viewBox="0 0 24 24" fill="currentColor">
      <path d="M19 3H5L2 9l10 12L22 9l-3-6zM9.62 8l1.5-3h1.76l1.5 3H9.62zM11 10v6.68L5.44 10H11zm2 0h5.56L13 16.68V10zm6.26-2h-2.65l-1.5-3h2.65l1.5 3zM6.24 5h2.65l-1.5 3H4.74l1.5-3z"/>
    </svg>
    <svg v-else-if="tier === 'pro'" class="w-3 h-3" viewBox="0 0 24 24" fill="currentColor">
      <path d="M5 16L3 5l5.5 5L12 4l3.5 6L21 5l-2 11H5zm14 3c0 .6-.4 1-1 1H6c-.6 0-1-.4-1-1v-1h14v1z"/>
    </svg>
    <svg v-else-if="tier === 'pro plus'" class="w-3 h-3" viewBox="0 0 24 24" fill="currentColor">
      <path d="M12 2l2.4 7.2L22 12l-7.6 2.8L12 22l-2.4-7.2L2 12l7.6-2.8z"/>
    </svg>
    {{ membershipType }}
  </span>
</template>

<script setup>
import { computed } from 'vue'

/** Cursor 套餐徽章：图标按套餐等级区分，样式由调用方（useCursorAccount.getMembershipBadgeClass）给出 */
const props = defineProps({
  membershipType: { type: String, default: '' },
  badgeClass: { type: [String, Array, Object], default: 'badge badge--sm uppercase' }
})

const tier = computed(() => props.membershipType?.toLowerCase() ?? '')
</script>
