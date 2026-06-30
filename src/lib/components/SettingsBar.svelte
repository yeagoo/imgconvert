<!-- SPDX-License-Identifier: Apache-2.0 -->
<script lang="ts">
  import { open } from "@tauri-apps/plugin-dialog";
  import { FolderOpen, ArrowsClockwise } from "phosphor-svelte";
  import * as Select from "$lib/components/ui/select";
  import { Slider } from "$lib/components/ui/slider";
  import { Switch } from "$lib/components/ui/switch";
  import { Label } from "$lib/components/ui/label";
  import { Button } from "$lib/components/ui/button";
  import FormatSelect from "$lib/components/FormatSelect.svelte";
  import {
    extOf,
    formatFromExt,
    isTauriRuntime,
    persistSettings,
    queue,
    resetItemFormats,
    settings,
    supportsLossless,
    ui,
  } from "$lib/state.svelte";

  let outputMessage = $state("");
  const busy = $derived(ui.converting || ui.importing);

  function isString(value: string | null): value is string {
    return value !== null;
  }

  const sourceFormats = $derived(
    queue.map((item) => formatFromExt(extOf(item.path))).filter(isString),
  );
  const canLossless = $derived(supportsLossless(settings.format));
  const lossyEnabled = $derived(!(canLossless && settings.lossless));
  const overwriteLabel = $derived(
    settings.overwrite === "overwrite"
      ? "覆盖"
      : settings.overwrite === "ask"
        ? "询问"
        : "跳过",
  );

  // 切到不支持无损的格式时,自动关掉无损开关
  $effect(() => {
    if (!canLossless && settings.lossless) {
      settings.lossless = false;
      persistSettings();
    }
  });

  $effect(() => {
    if (settings.preserveMetadata) {
      settings.preserveMetadata = false;
      persistSettings();
    }
  });

  async function pickOut() {
    if (busy) return;

    if (!isTauriRuntime()) {
      outputMessage = "网页预览不支持选择本机输出目录";
      return;
    }

    const sel = await open({ directory: true, multiple: false });
    if (busy) return;

    if (typeof sel === "string") {
      settings.outDir = sel;
      outputMessage = "";
      persistSettings();
    }
  }

  function clearOut() {
    if (busy) return;

    settings.outDir = null;
    outputMessage = "";
    persistSettings();
  }

  function setFormat(value: string) {
    if (busy) return;

    settings.format = value;
    persistSettings();
  }

  function setOverwrite(value: string) {
    if (busy) return;

    if (value === "ask" || value === "skip" || value === "overwrite") {
      settings.overwrite = value;
    }
    persistSettings();
  }

  function setTemplate(value: string) {
    if (busy) return;

    settings.fileNameTemplate = value;
    persistSettings();
  }

</script>

<section class="rounded-lg border bg-card p-4">
  <div class="grid gap-4 lg:grid-cols-[220px_minmax(260px,1fr)_220px_220px]">
  <div class="flex flex-col gap-1.5">
    <Label class="text-xs text-muted-foreground">目标格式</Label>
    <FormatSelect
      bind:value={settings.format}
      {sourceFormats}
      onChange={setFormat}
      disabled={busy}
      triggerClass="w-full"
    />
  </div>

  <div class="flex min-w-0 flex-col gap-2" class:opacity-40={!lossyEnabled || busy}>
    <div class="flex items-center justify-between gap-3">
      <Label class="text-xs text-muted-foreground">质量</Label>
      <b class="tabular-nums text-sm text-foreground">{settings.quality}</b>
    </div>
    <div class="flex h-8 items-center">
      <Slider
        type="single"
        bind:value={settings.quality}
        min={1}
        max={100}
        step={1}
        disabled={!lossyEnabled || busy}
        onValueCommit={persistSettings}
      />
    </div>
  </div>

  <div class="flex min-w-0 flex-col gap-1.5">
    <Label class="text-xs text-muted-foreground">已存在文件</Label>
    <Select.Root
      type="single"
      bind:value={settings.overwrite}
      disabled={busy}
      onValueChange={setOverwrite}
    >
      <Select.Trigger class="w-full" disabled={busy}>{overwriteLabel}</Select.Trigger>
      <Select.Content>
        <Select.Item value="ask" label="询问">询问</Select.Item>
        <Select.Item value="skip" label="跳过">跳过</Select.Item>
        <Select.Item value="overwrite" label="覆盖">覆盖</Select.Item>
      </Select.Content>
    </Select.Root>
  </div>

  <div class="flex min-w-0 flex-col gap-1.5">
    <Label class="text-xs text-muted-foreground">文件名模板</Label>
    <input
      value={settings.fileNameTemplate}
      class="h-8 rounded-md border bg-background px-2 text-sm outline-none focus-visible:border-ring focus-visible:ring-3 focus-visible:ring-ring/50"
      placeholder="%name%"
      disabled={busy}
      oninput={(event) => setTemplate(event.currentTarget.value)}
    />
  </div>

  <div class="flex min-w-0 items-center gap-5 lg:col-span-2">
    <div class="flex items-center gap-2" class:opacity-40={!canLossless || busy}>
      <Switch
        bind:checked={settings.lossless}
        disabled={!canLossless || busy}
        onCheckedChange={persistSettings}
      />
      <Label class="text-sm">无损压缩</Label>
    </div>

    <div class="flex items-center gap-2 opacity-40" title="P2 实现后启用">
      <Switch
        bind:checked={settings.preserveMetadata}
        disabled
        onCheckedChange={persistSettings}
      />
      <Label class="text-sm">保留元数据</Label>
    </div>
  </div>

  <div class="flex items-center gap-2">
    <Button
      variant="ghost"
      size="sm"
      onclick={resetItemFormats}
      disabled={busy || !queue.length}
    >
      <ArrowsClockwise weight="duotone" />
      跟随全局格式
    </Button>
  </div>

  <div class="flex min-w-0 items-center gap-2">
    <Button variant="ghost" size="sm" onclick={pickOut} disabled={busy}>
      <FolderOpen weight="duotone" />
      输出目录
    </Button>
    <button
      class="max-w-[220px] truncate text-left text-xs text-muted-foreground hover:text-foreground"
      title={settings.outDir ?? "与原文件相同目录(点击清除自定义目录)"}
      onclick={clearOut}
      disabled={busy}
    >
      {settings.outDir ?? "与原文件相同目录"}
    </button>
    {#if outputMessage}
      <span class="text-xs text-muted-foreground">{outputMessage}</span>
    {/if}
  </div>
  </div>
</section>
