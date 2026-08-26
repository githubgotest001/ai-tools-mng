<template>
  <!-- 无标签：表格里是图标按钮，卡片里是带文字的虚线胶囊，沿用各自原来的样式 -->
  <span
    v-if="!tags.length && emptyStyle === 'icon'"
    class="btn btn--icon-sm btn--dashed"
    v-tooltip="$t('tokenList.clickToAddTag')"
    @click.stop="$emit('edit')"
  >
    <svg width="14" height="14" viewBox="0 0 24 24" fill="currentColor">
      <path d="M19 13h-6v6h-2v-6H5v-2h6V5h2v6h6v2z"/>
    </svg>
  </span>
  <span
    v-else-if="!tags.length"
    class="inline-flex items-center gap-0.5 px-1.5 py-0.5 border border-dashed border-border rounded text-text-muted text-xs cursor-pointer hover:border-accent hover:text-accent transition-colors"
    @click.stop="$emit('edit')"
  >
    <svg width="12" height="12" viewBox="0 0 24 24" fill="currentColor">
      <path d="M19 13h-6v6h-2v-6H5v-2h6V5h2v6h6v2z"/>
    </svg>
    {{ $t('tokenList.clickToAddTag') }}
  </span>

  <!-- 有标签：只渲染前 max 个，其余折进「+N」，永远单行不换行 -->
  <span
    v-else
    class="inline-flex items-center gap-1 max-w-full align-middle cursor-pointer"
    v-tooltip="tooltip"
    @click.stop="$emit('edit')"
  >
    <span
      v-for="tag in visibleTags"
      :key="tag.name"
      class="badge editable min-w-0 truncate"
      :class="[sizeClass, badgeClass]"
      :style="{ '--tag-color': tag.color }"
    >
      {{ tag.name }}
    </span>
    <span
      v-if="hiddenCount > 0"
      class="badge shrink-0 tabular-nums"
      :class="sizeClass"
    >
      +{{ hiddenCount }}
    </span>
  </span>
</template>

<script setup>
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'
import { accountTags } from '../../utils/accountTags'

const { t: $t } = useI18n()

const props = defineProps({
  /** 账号对象，直接读它的 tags（兼容旧的 tag / tag_color） */
  account: { type: Object, default: null },
  /** 也可以直接传标签列表，用于预览等没有账号对象的场景 */
  modelValue: { type: Array, default: null },
  /**
   * 最多显示几个标签，其余折成「+N」。
   *
   * 表格的标签列只有 50~140px，多一个 badge 就会把列撑开或挤掉邮箱列，
   * 所以窄处传 1、卡片传 2，永远只占一行，行高不会因标签数量变化。
   */
  max: { type: Number, default: 1 },
  size: { type: String, default: 'sm' },
  /** 各处自带的宽度约束（如 max-w-[120px]），原样透传给标签 badge */
  badgeClass: { type: String, default: '' },
  /** 无标签时的入口样式：表格用 icon，卡片用带文字的 text */
  emptyStyle: { type: String, default: 'icon' }
})

defineEmits(['edit'])

const tags = computed(() =>
  props.modelValue ? props.modelValue : accountTags(props.account)
)

const sizeClass = computed(() => (props.size === 'sm' ? 'badge--sm' : ''))

const visibleTags = computed(() => tags.value.slice(0, Math.max(1, props.max)))

const hiddenCount = computed(() => tags.value.length - visibleTags.value.length)

// 折叠掉的标签只能靠 tooltip 看到，所以这里始终列出全部名字
const tooltip = computed(() =>
  [tags.value.map((tag) => tag.name).join(' · '), $t('tokenList.clickToEditTag')].join(' · ')
)
</script>
