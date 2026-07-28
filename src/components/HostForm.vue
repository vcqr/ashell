<script setup lang="ts">
import { computed, h, onMounted, reactive, ref, watch } from "vue"
import {
  NForm,
  NFormItem,
  NInput,
  NInputGroup,
  NSelect,
  NColorPicker,
  NButton,
  NSpace,
  NGrid,
  NGi,
  type FormInst,
  type FormRules,
  type SelectOption,
  type SelectRenderTag,
} from "naive-ui"
import { useI18n } from "vue-i18n"
import { invoke } from "@tauri-apps/api/core"
import { useHostStore } from "@/stores/hosts"
import { useIconStore } from "@/stores/icons"
import type { Host, HostCreate, HostUpdate, Group, HostProtocol } from "@/types"

const props = defineProps<{
  mode: "create" | "edit"
  initial?: Host | null
  defaultGid?: number
}>()

const emit = defineEmits<{
  submit: [data: HostCreate | HostUpdate]
  cancel: []
}>()

const { t } = useI18n()
const store = useHostStore()
const formRef = ref<FormInst | null>(null)

interface FormState {
  gid: number
  name: string
  addr: string
  port: string
  username: string
  password: string
  private_key: string
  private_key_path: string
  /** NSelect clearable 清除后值为 null */
  icon: string | null
  color: string
  desc: string
  protocol: HostProtocol
  baud_rate: number
  data_bits: number
  stop_bits: number
  parity: string
  flow_control: string
}

function makeInitial(): FormState {
  const init = props.initial
  return {
    gid: init?.gid ?? props.defaultGid ?? 0,
    name: init?.name ?? "",
    addr: init?.addr ?? "",
    port: init?.port ?? "22",
    username: init?.username ?? "",
    password: "",
    private_key: "",
    private_key_path: init?.private_key_path ?? "",
    icon: init?.icon ?? "",
    color: init?.color ?? "",
    desc: init?.desc ?? "",
    protocol: init?.protocol ?? "ssh",
    baud_rate: init?.baud_rate ?? 9600,
    data_bits: init?.data_bits ?? 8,
    stop_bits: init?.stop_bits ?? 1,
    parity: init?.parity ?? "none",
    flow_control: init?.flow_control ?? "none",
  }
}

const form = reactive<FormState>(makeInitial())

watch(
  () => [props.initial, props.defaultGid, props.mode] as const,
  () => {
    Object.assign(form, makeInitial())
  },
)

const groupOptions = computed<SelectOption[]>(() => {
  const opts: SelectOption[] = [{ label: t("common.rootDir"), value: 0 }]
  const sorted = [...store.groups].sort((a: Group, b: Group) => {
    if (a.level !== b.level) return a.level - b.level
    return a.name.localeCompare(b.name)
  })
  for (const g of sorted) {
    const indent = "\u00a0\u00a0".repeat(Math.max(0, g.level))
    opts.push({ label: `${indent}${g.name}`, value: g.id })
  }
  return opts
})

/* ---------- 图标选项（来自 ~/.ashell/icons/） ---------- */
const iconStore = useIconStore()

onMounted(() => {
  void iconStore.ensureLoaded()
})

const iconOptions = computed<SelectOption[]>(() => {
  const opts: SelectOption[] = [{ label: t("common.none"), value: "" }]
  for (const it of iconStore.items) {
    opts.push({ label: it.name, value: it.name })
  }
  return opts
})

function renderIconThumb(name: string, size = 18) {
  const url = iconStore.urlOf(name)
  if (!url) return null
  return h("img", {
    src: url,
    width: size,
    height: size,
    style: {
      borderRadius: "3px",
      objectFit: "contain",
      flexShrink: 0,
    },
  })
}

function renderIconOption(option: SelectOption) {
  const name = option.value as string
  return h(
    "div",
    {
      style: {
        display: "flex",
        alignItems: "center",
        gap: "8px",
        minHeight: "22px",
      },
    },
    [
      renderIconThumb(name),
      h("span", { style: { fontSize: "13px" } }, option.label as string),
    ],
  )
}

const renderIconTag: SelectRenderTag = ({ option }) => {
  const name = option.value as string
  return h(
    "div",
    {
      style: {
        display: "flex",
        alignItems: "center",
        gap: "8px",
      },
    },
    [
      renderIconThumb(name, 16),
      h("span", null, option.label as string),
    ],
  )
}

const isSerial = computed(() => form.protocol === "serial")
const isSsh = computed(() => form.protocol === "ssh")

const rules = computed<FormRules>(() => ({
  name: [{ required: true, message: t("hosts.form.rules.nameRequired"), trigger: ["blur", "input"] }],
  addr: [{ required: true, message: t("hosts.form.rules.addrRequired"), trigger: ["blur", "input"] }],
  port: isSerial.value
    ? []
    : [
        {
          required: true,
          validator(_rule, value: string) {
            if (!value) return new Error(t("hosts.form.rules.portRequired"))
            const n = Number(value)
            if (!Number.isInteger(n) || n <= 0 || n > 65535) {
              return new Error(t("hosts.form.rules.portRange"))
            }
            return true
          },
          trigger: ["blur", "input"],
        },
      ],
  username: isSerial.value
    ? []
    : [
        { required: true, message: t("hosts.form.rules.usernameRequired"), trigger: ["blur", "input"] },
      ],
}))

function emptyToNull(v: string | null | undefined): string | null {
  if (v == null) return null
  return v.trim().length === 0 ? null : v
}

async function pickPrivateKeyFile() {
  try {
    const path = await invoke<string | null>("pick_private_key_file")
    if (path) {
      form.private_key_path = path
    }
  } catch {
    // 用户取消或对话框错误，忽略
  }
}

function clearPrivateKeyPath() {
  form.private_key_path = ""
}

async function submit() {
  if (!formRef.value) return
  try {
    await formRef.value.validate()
  } catch {
    return
  }

  if (props.mode === "create") {
    const payload: HostCreate = {
      gid: form.gid,
      name: form.name.trim(),
      addr: form.addr.trim(),
      port: form.port.trim(),
      username: form.username.trim(),
      password: emptyToNull(form.password),
      private_key: emptyToNull(form.private_key),
      private_key_path: emptyToNull(form.private_key_path),
      icon: emptyToNull(form.icon),
      color: emptyToNull(form.color),
      desc: emptyToNull(form.desc),
      protocol: form.protocol,
      baud_rate: isSerial.value ? form.baud_rate : null,
      data_bits: isSerial.value ? form.data_bits : null,
      stop_bits: isSerial.value ? form.stop_bits : null,
      parity: isSerial.value ? form.parity : null,
      flow_control: isSerial.value ? form.flow_control : null,
    }
    emit("submit", payload)
  } else {
    const payload: HostUpdate = {
      gid: form.gid,
      name: form.name.trim(),
      addr: form.addr.trim(),
      port: form.port.trim(),
      username: form.username.trim(),
      icon: emptyToNull(form.icon),
      color: emptyToNull(form.color),
      desc: emptyToNull(form.desc),
      protocol: form.protocol,
      baud_rate: isSerial.value ? form.baud_rate : null,
      data_bits: isSerial.value ? form.data_bits : null,
      stop_bits: isSerial.value ? form.stop_bits : null,
      parity: isSerial.value ? form.parity : null,
      flow_control: isSerial.value ? form.flow_control : null,
    }
    if (form.password.length > 0) payload.password = form.password
    if (form.private_key.length > 0) payload.private_key = form.private_key
    payload.private_key_path = emptyToNull(form.private_key_path)
    emit("submit", payload)
  }
}

function cancel() {
  emit("cancel")
}
</script>

<template>
  <NForm
    ref="formRef"
    :model="form"
    :rules="rules"
    label-placement="top"
    require-mark-placement="right-hanging"
    class="host-form"
  >
    <div class="host-form-columns">
      <!-- 左栏：名称、目录、图标、颜色、描述 -->
      <div class="host-form-col">
        <div class="form-section-title">{{ t("hosts.form.sectionBasic") }}</div>

        <NFormItem :label="t('hosts.form.name')" path="name">
          <NInput v-model:value="form.name" :placeholder="t('hosts.form.namePlaceholder')" />
        </NFormItem>

        <NFormItem :label="t('hosts.form.group')" path="gid">
          <NSelect
            v-model:value="form.gid"
            :options="groupOptions"
            filterable
            :placeholder="t('hosts.form.groupPlaceholder')"
          />
        </NFormItem>

        <NFormItem :label="t('hosts.form.icon')" path="icon">
          <NSelect
            v-model:value="form.icon"
            :options="iconOptions"
            :render-label="renderIconOption"
            :render-tag="renderIconTag"
            filterable
            clearable
            :placeholder="t('hosts.form.iconPlaceholder')"
          />
        </NFormItem>

        <NFormItem :label="t('hosts.form.color')" path="color">
          <NColorPicker
            v-model:value="form.color"
            :modes="['hex']"
            :show-alpha="false"
            style="width: 100%"
          />
        </NFormItem>

        <NFormItem :label="t('hosts.form.desc')" path="desc">
          <NInput
            v-model:value="form.desc"
            type="textarea"
            :autosize="{ minRows: 2, maxRows: 4 }"
            :placeholder="t('hosts.form.descPlaceholder')"
          />
        </NFormItem>
      </div>

      <!-- 右栏：协议、地址、端口、用户名、密码、私钥、私钥文件 -->
      <div class="host-form-col">
        <div class="form-section-title">{{ t("hosts.form.sectionConn") }}</div>

        <NFormItem :label="t('hosts.form.protocol')" path="protocol">
          <NSelect
            v-model:value="form.protocol"
            :options="[
              { label: 'SSH', value: 'ssh' },
              { label: 'Telnet', value: 'telnet' },
              { label: t('hosts.form.protocolSerial'), value: 'serial' },
            ]"
          />
        </NFormItem>

        <template v-if="!isSerial">
          <NGrid :cols="3" :x-gap="12">
            <NGi :span="2">
              <NFormItem :label="t('hosts.form.addr')" path="addr">
                <NInput v-model:value="form.addr" :placeholder="t('hosts.form.addrPlaceholder')" />
              </NFormItem>
            </NGi>
            <NGi>
              <NFormItem :label="t('hosts.form.port')" path="port">
                <NInput v-model:value="form.port" :placeholder="form.protocol === 'telnet' ? '23' : t('hosts.form.portPlaceholder')" />
              </NFormItem>
            </NGi>
          </NGrid>

          <NFormItem :label="t('hosts.form.username')" path="username">
            <NInput v-model:value="form.username" :placeholder="t('hosts.form.usernamePlaceholder')" />
          </NFormItem>

          <NFormItem :label="t('hosts.form.password')" path="password">
            <NInput
              v-model:value="form.password"
              type="password"
              show-password-on="click"
              :placeholder="props.mode === 'edit' ? t('hosts.form.passwordPlaceholder') : t('hosts.form.passwordOptional')"
            />
          </NFormItem>

          <template v-if="isSsh">
            <NFormItem :label="t('hosts.form.privateKey')" path="private_key">
              <NInput
                v-model:value="form.private_key"
                type="textarea"
                :autosize="{ minRows: 3, maxRows: 6 }"
                :placeholder="
                  props.mode === 'edit'
                    ? t('hosts.form.privateKeyPlaceholder')
                    : t('hosts.form.privateKeyOptional')
                "
              />
            </NFormItem>

            <NFormItem :label="t('hosts.form.privateKeyPath')" path="private_key_path">
              <NInputGroup>
                <NInput
                  v-model:value="form.private_key_path"
                  :placeholder="t('hosts.form.privateKeyPathPlaceholder')"
                />
                <NButton @click="pickPrivateKeyFile">{{ t("hosts.form.selectFile") }}</NButton>
                <NButton
                  v-if="form.private_key_path"
                  quaternary
                  @click="clearPrivateKeyPath"
                >
                  {{ t("hosts.form.clearFile") }}
                </NButton>
              </NInputGroup>
            </NFormItem>
          </template>
        </template>

        <template v-else>
          <NFormItem :label="t('hosts.form.serialPath')" path="addr">
            <NInput v-model:value="form.addr" :placeholder="t('hosts.form.serialPathPlaceholder')" />
          </NFormItem>

          <NGrid :cols="3" :x-gap="12">
            <NGi>
              <NFormItem :label="t('hosts.form.baudRate')" path="baud_rate">
                <NSelect
                  v-model:value="form.baud_rate"
                  :options="[
                    { label: '1200', value: 1200 },
                    { label: '2400', value: 2400 },
                    { label: '4800', value: 4800 },
                    { label: '9600', value: 9600 },
                    { label: '19200', value: 19200 },
                    { label: '38400', value: 38400 },
                    { label: '57600', value: 57600 },
                    { label: '115200', value: 115200 },
                  ]"
                />
              </NFormItem>
            </NGi>
            <NGi>
              <NFormItem :label="t('hosts.form.dataBits')" path="data_bits">
                <NSelect
                  v-model:value="form.data_bits"
                  :options="[
                    { label: '5', value: 5 },
                    { label: '6', value: 6 },
                    { label: '7', value: 7 },
                    { label: '8', value: 8 },
                  ]"
                />
              </NFormItem>
            </NGi>
            <NGi>
              <NFormItem :label="t('hosts.form.stopBits')" path="stop_bits">
                <NSelect
                  v-model:value="form.stop_bits"
                  :options="[
                    { label: '1', value: 1 },
                    { label: '2', value: 2 },
                  ]"
                />
              </NFormItem>
            </NGi>
          </NGrid>

          <NGrid :cols="2" :x-gap="12">
            <NGi>
              <NFormItem :label="t('hosts.form.parity')" path="parity">
                <NSelect
                  v-model:value="form.parity"
                  :options="[
                    { label: t('hosts.form.parityNone'), value: 'none' },
                    { label: t('hosts.form.parityOdd'), value: 'odd' },
                    { label: t('hosts.form.parityEven'), value: 'even' },
                  ]"
                />
              </NFormItem>
            </NGi>
            <NGi>
              <NFormItem :label="t('hosts.form.flowControl')" path="flow_control">
                <NSelect
                  v-model:value="form.flow_control"
                  :options="[
                    { label: t('hosts.form.flowNone'), value: 'none' },
                    { label: t('hosts.form.flowSoftware'), value: 'software' },
                    { label: t('hosts.form.flowHardware'), value: 'hardware' },
                  ]"
                />
              </NFormItem>
            </NGi>
          </NGrid>
        </template>
      </div>
    </div>

    <NSpace justify="end">
      <NButton @click="cancel">{{ t("hosts.form.cancel") }}</NButton>
      <NButton type="primary" @click="submit">
        {{ props.mode === "create" ? t("hosts.form.create") : t("hosts.form.save") }}
      </NButton>
    </NSpace>
  </NForm>
</template>

<style scoped>
.host-form {
  padding: 4px 2px 0;
}
.host-form-columns {
  display: flex;
}
.host-form-col {
  flex: 1;
  min-width: 0;
}
.host-form-col:first-child {
  flex: 0 0 35%;
  border-right: 1px solid var(--ashell-border);
  padding-right: 16px;
}
.host-form-col:last-child {
  flex: 1;
  padding-left: 16px;
}
.form-section-title {
  font-size: 13px;
  font-weight: 600;
  color: var(--ashell-text-strong);
  margin-bottom: 8px;
}
</style>
