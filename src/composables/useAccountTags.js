import { computed, ref } from 'vue'
import { useI18n } from 'vue-i18n'

import { accountTags, accountsAsTagTokens, setAccountTags } from '../utils/accountTags'

/**
 * 账号卡片 / 表格行的标签编辑逻辑。
 *
 * 5 个平台的 Card 与 TableRow（共 10 个组件）此前各自维护一份逐字相同的实现，
 * 抽到这里以免多标签的读写规则再次走散。
 *
 * @param props 组件 props，需含 `account` 与 `allAccounts`
 * @param emit  组件 emit，保存后触发 `account-updated`
 */
export function useAccountTags(props, emit) {
  const { t } = useI18n()

  const showTagEditor = ref(false)

  // TagEditorModal 认的是 token 形状，这里把账号包一层
  const accountAsToken = computed(() => ({ tags: accountTags(props.account) }))

  // 标签建议来自全部账号，而不只是当前这条
  const allAccountsAsTokens = computed(() => accountsAsTagTokens(props.allAccounts))

  const openTagEditor = () => {
    showTagEditor.value = true
  }

  const handleTagSave = ({ tags }) => {
    setAccountTags(props.account, tags)
    emit('account-updated', props.account)
    window.$notify?.success(t('messages.tagUpdated'))
  }

  const handleTagClear = () => {
    setAccountTags(props.account, [])
    emit('account-updated', props.account)
    window.$notify?.success(t('messages.tagCleared'))
  }

  return {
    showTagEditor,
    accountAsToken,
    allAccountsAsTokens,
    openTagEditor,
    handleTagSave,
    handleTagClear
  }
}
