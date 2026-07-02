<!-- SPDX-License-Identifier: Apache-2.0 -->
<script lang="ts">
  import { FolderOpen, ArrowsClockwise } from "phosphor-svelte";
  import * as Select from "$lib/components/ui/select";
  import { Slider } from "$lib/components/ui/slider";
  import { Switch } from "$lib/components/ui/switch";
  import { Label } from "$lib/components/ui/label";
  import { Button } from "$lib/components/ui/button";
  import FormatSelect from "$lib/components/FormatSelect.svelte";
  import {
    effectiveQualityFor,
    extOf,
    formatFromExt,
    isTauriRuntime,
    pickSystemPaths,
    persistSettings,
    qualityFloorFor,
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
  const qualityEnabled = $derived(settings.format !== "png" && !(canLossless && settings.lossless));
  const autoQualitySupported = $derived(
    ["jpeg", "webp"].includes(settings.format) && qualityEnabled,
  );
  const qualityTitle = $derived(settings.autoQuality && autoQualitySupported ? "质量上限" : "质量");
  const hasQualityFloor = $derived(["jpeg", "webp", "avif"].includes(settings.format));
  const activeQualityFloor = $derived(qualityFloorFor(settings.format));
  const activeQualityFloorLabel = $derived(
    activeQualityFloor >= 30 ? `${activeQualityFloor}` : "关闭",
  );
  const effectiveQuality = $derived(effectiveQualityFor(settings.format));
  const qualityLabel = $derived(
    qualityEnabled && effectiveQuality > settings.quality
      ? `${settings.quality} -> ${effectiveQuality}`
      : `${settings.quality}`,
  );
  const concurrencyLabel = $derived(settings.concurrency > 0 ? `${settings.concurrency}` : "自动");
  const autoQualityScoreLabel = $derived(`${settings.autoQualityScore}`);
  const overwriteLabel = $derived(
    settings.overwrite === "overwrite" ? "覆盖" : settings.overwrite === "ask" ? "询问" : "跳过",
  );
  const avifSubsampleLabel = $derived(settings.avifSubsample === "yuv420" ? "4:2:0" : "4:4:4");

  // 切到不支持无损的格式时,自动关掉无损开关
  $effect(() => {
    if (!canLossless && settings.lossless) {
      settings.lossless = false;
      persistSettings();
    }
  });

  async function pickOut() {
    if (busy) return;

    if (!isTauriRuntime()) {
      outputMessage = "网页预览不支持选择本机输出目录";
      return;
    }

    try {
      const paths = await pickSystemPaths({
        directory: true,
        multiple: false,
        title: "选择输出目录",
      });
      if (busy || !paths[0]) return;
      settings.outDir = paths[0];
      outputMessage = "";
      persistSettings();
    } catch (error) {
      outputMessage = String(error);
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

  function commitConcurrency() {
    if (busy) return;

    settings.concurrency = Math.min(8, Math.max(0, Math.round(settings.concurrency)));
    persistSettings();
  }

  function clampQualityFloor(value: number) {
    const rounded = Math.min(100, Math.max(0, Math.round(value)));
    return rounded < 30 ? 0 : rounded;
  }

  function commitJpegQualityFloor() {
    if (busy) return;

    settings.jpegQualityFloor = clampQualityFloor(settings.jpegQualityFloor);
    persistSettings();
  }

  function commitWebpQualityFloor() {
    if (busy) return;

    settings.webpQualityFloor = clampQualityFloor(settings.webpQualityFloor);
    persistSettings();
  }

  function commitAvifQualityFloor() {
    if (busy) return;

    settings.avifQualityFloor = clampQualityFloor(settings.avifQualityFloor);
    persistSettings();
  }

  function commitPngOxipngLevel() {
    if (busy) return;

    settings.pngOxipngLevel = Math.min(6, Math.max(0, Math.round(settings.pngOxipngLevel)));
    persistSettings();
  }

  function commitPngQuantColors() {
    if (busy) return;

    settings.pngQuantColors = Math.min(256, Math.max(64, Math.round(settings.pngQuantColors)));
    persistSettings();
  }

  function commitWebpMethod() {
    if (busy) return;

    settings.webpMethod = Math.min(6, Math.max(0, Math.round(settings.webpMethod)));
    persistSettings();
  }

  function commitAvifSpeed() {
    if (busy) return;

    settings.avifSpeed = Math.min(10, Math.max(0, Math.round(settings.avifSpeed)));
    persistSettings();
  }

  function commitWebpNearLossless() {
    if (busy) return;

    settings.webpNearLossless = Math.min(100, Math.max(0, Math.round(settings.webpNearLossless)));
    persistSettings();
  }

  function commitAutoQualityScore() {
    if (busy) return;

    settings.autoQualityScore = Math.min(95, Math.max(50, Math.round(settings.autoQualityScore)));
    persistSettings();
  }

  function setAvifSubsample(value: string) {
    if (busy) return;

    if (value === "yuv444" || value === "yuv420") {
      settings.avifSubsample = value;
      persistSettings();
    }
  }
</script>

<section class="rounded-lg border bg-card p-4">
  <div class="grid gap-4 lg:grid-cols-[220px_minmax(220px,1fr)_180px_180px_220px]">
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

    <div class="flex min-w-0 flex-col gap-2" class:opacity-40={!qualityEnabled || busy}>
      <div class="flex items-center justify-between gap-3">
        <Label class="text-xs text-muted-foreground">{qualityTitle}</Label>
        <b class="tabular-nums text-sm text-foreground">{qualityLabel}</b>
      </div>
      <div class="flex h-8 items-center">
        <Slider
          type="single"
          bind:value={settings.quality}
          min={1}
          max={100}
          step={1}
          disabled={!qualityEnabled || busy}
          onValueCommit={persistSettings}
        />
      </div>
    </div>

    <div class="flex min-w-0 flex-col gap-2" class:opacity-40={busy}>
      <div class="flex items-center justify-between gap-3">
        <Label class="text-xs text-muted-foreground">并发</Label>
        <b class="tabular-nums text-sm text-foreground">{concurrencyLabel}</b>
      </div>
      <div class="flex h-8 items-center">
        <Slider
          type="single"
          bind:value={settings.concurrency}
          min={0}
          max={8}
          step={1}
          disabled={busy}
          onValueCommit={commitConcurrency}
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

    <div class="flex min-w-0 flex-wrap items-center gap-x-5 gap-y-2 lg:col-span-3">
      <div class="flex items-center gap-2" class:opacity-40={!canLossless || busy}>
        <Switch
          bind:checked={settings.lossless}
          disabled={!canLossless || busy}
          onCheckedChange={persistSettings}
        />
        <Label class="text-sm">无损压缩</Label>
      </div>

      <div
        class="flex items-center gap-2"
        class:opacity-40={busy}
        title="候选输出不小于源文件时跳过"
      >
        <Switch
          bind:checked={settings.skipIfLarger}
          disabled={busy}
          onCheckedChange={persistSettings}
        />
        <Label class="text-sm">跳过变大输出</Label>
      </div>

      <div
        class="flex items-center gap-2"
        class:opacity-40={busy}
        title="尝试多个等价编码候选并写入最小结果"
      >
        <Switch
          bind:checked={settings.multiCandidate}
          disabled={busy}
          onCheckedChange={persistSettings}
        />
        <Label class="text-sm">多候选取最小</Label>
      </div>

      <div
        class="flex items-center gap-2"
        class:opacity-40={!autoQualitySupported || busy}
        title="JPEG/WebP 用 SSIMULACRA2 搜索达标的最低质量"
      >
        <Switch
          checked={settings.autoQuality && autoQualitySupported}
          disabled={!autoQualitySupported || busy}
          onCheckedChange={(checked) => {
            settings.autoQuality = checked;
            persistSettings();
          }}
        />
        <Label class="text-sm">自动质量</Label>
      </div>

      <div
        class="flex items-center gap-2"
        class:opacity-40={busy}
        title="有损源再次输出有损格式时要求足够体积收益"
      >
        <Switch
          bind:checked={settings.generationLossProtection}
          disabled={busy}
          onCheckedChange={persistSettings}
        />
        <Label class="text-sm">代际防护</Label>
      </div>

      <div
        class="flex items-center gap-2"
        class:opacity-40={busy}
        title="源文件与设置未变时复用已有输出"
      >
        <Switch
          bind:checked={settings.resultCache}
          disabled={busy}
          onCheckedChange={persistSettings}
        />
        <Label class="text-sm">结果缓存</Label>
      </div>

      <div
        class="flex items-center gap-2"
        class:opacity-40={busy}
        title="保留 ICC 色彩配置和 EXIF 元数据"
      >
        <Switch
          bind:checked={settings.preserveMetadata}
          disabled={busy}
          onCheckedChange={persistSettings}
        />
        <Label class="text-sm">保留元数据</Label>
      </div>
    </div>

    <div class="flex items-center gap-2">
      <Button variant="ghost" size="sm" onclick={resetItemFormats} disabled={busy || !queue.length}>
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

    <div class="flex min-w-0 flex-col gap-2 border-t pt-3 lg:col-span-5">
      <div class="flex items-center justify-between gap-3">
        <Label class="text-xs text-muted-foreground">格式参数</Label>
        {#if settings.format === "jpeg"}
          <span class="text-xs text-muted-foreground">
            {settings.jpegProgressive ? "Progressive" : "Baseline"}
          </span>
        {:else if settings.format === "png"}
          <span class="text-xs text-muted-foreground">oxipng {settings.pngOxipngLevel}</span>
        {:else if settings.format === "webp"}
          <span class="text-xs text-muted-foreground">method {settings.webpMethod}</span>
        {:else if settings.format === "avif"}
          <span class="text-xs text-muted-foreground">speed {settings.avifSpeed}</span>
        {/if}
      </div>

      {#if settings.format === "jpeg"}
        <div class="flex min-h-8 flex-wrap items-center gap-x-5 gap-y-2">
          <div class="flex items-center gap-2">
            <Switch
              bind:checked={settings.jpegProgressive}
              disabled={busy}
              onCheckedChange={persistSettings}
            />
            <Label class="text-sm">Progressive JPEG</Label>
          </div>
          <div class="flex items-center gap-2">
            <Switch
              bind:checked={settings.jpegTrellis}
              disabled={busy}
              onCheckedChange={persistSettings}
            />
            <Label class="text-sm">Trellis scans</Label>
          </div>
        </div>
      {:else if settings.format === "png"}
        <div class="flex h-8 items-center gap-3">
          <Slider
            type="single"
            bind:value={settings.pngOxipngLevel}
            min={0}
            max={6}
            step={1}
            disabled={busy}
            onValueCommit={commitPngOxipngLevel}
          />
        </div>
        <div class="flex h-8 items-center gap-2" class:opacity-40={busy}>
          <Switch
            bind:checked={settings.pngLossyQuantize}
            disabled={busy}
            onCheckedChange={persistSettings}
          />
          <Label class="text-sm">实验性限色</Label>
        </div>
        {#if settings.pngLossyQuantize}
          <div class="grid gap-2 pt-1 md:grid-cols-[120px_minmax(0,1fr)]" class:opacity-40={busy}>
            <div class="flex items-center justify-between gap-3 md:block">
              <Label class="text-sm text-muted-foreground">颜色数</Label>
              <span class="tabular-nums text-xs text-muted-foreground md:mt-1 md:block">
                {settings.pngQuantColors}
              </span>
            </div>
            <div class="flex h-8 min-w-0 items-center">
              <Slider
                type="single"
                bind:value={settings.pngQuantColors}
                min={64}
                max={256}
                step={1}
                disabled={busy}
                onValueCommit={commitPngQuantColors}
              />
            </div>
          </div>
        {/if}
      {:else if settings.format === "webp"}
        <div class="flex h-8 items-center gap-3">
          <Slider
            type="single"
            bind:value={settings.webpMethod}
            min={0}
            max={6}
            step={1}
            disabled={busy}
            onValueCommit={commitWebpMethod}
          />
        </div>
        <div class="grid gap-2 pt-1 md:grid-cols-[120px_minmax(0,1fr)]" class:opacity-40={busy}>
          <div class="flex items-center justify-between gap-3 md:block">
            <Label class="text-sm text-muted-foreground">near-lossless</Label>
            <span class="tabular-nums text-xs text-muted-foreground md:mt-1 md:block">
              {settings.webpNearLossless === 100 ? "关闭" : settings.webpNearLossless}
            </span>
          </div>
          <div class="flex h-8 min-w-0 items-center">
            <Slider
              type="single"
              bind:value={settings.webpNearLossless}
              min={0}
              max={100}
              step={1}
              disabled={busy}
              onValueCommit={commitWebpNearLossless}
            />
          </div>
        </div>
        <div class="flex h-8 items-center gap-2" class:opacity-40={busy}>
          <Switch
            bind:checked={settings.webpSharpYuv}
            disabled={busy}
            onCheckedChange={persistSettings}
          />
          <Label class="text-sm">Sharp YUV</Label>
        </div>
      {:else if settings.format === "avif"}
        <div class="grid gap-3 md:grid-cols-[minmax(0,1fr)_140px]">
          <div class="flex h-8 items-center gap-3">
            <Slider
              type="single"
              bind:value={settings.avifSpeed}
              min={0}
              max={10}
              step={1}
              disabled={busy}
              onValueCommit={commitAvifSpeed}
            />
          </div>
          <Select.Root
            type="single"
            bind:value={settings.avifSubsample}
            disabled={busy}
            onValueChange={setAvifSubsample}
          >
            <Select.Trigger class="h-8 w-full" disabled={busy}>{avifSubsampleLabel}</Select.Trigger>
            <Select.Content>
              <Select.Item value="yuv444" label="4:4:4">4:4:4</Select.Item>
              <Select.Item value="yuv420" label="4:2:0">4:2:0</Select.Item>
            </Select.Content>
          </Select.Root>
        </div>
      {/if}

      {#if autoQualitySupported && settings.autoQuality}
        <div class="grid gap-2 pt-1 md:grid-cols-[120px_minmax(0,1fr)]" class:opacity-40={busy}>
          <div class="flex items-center justify-between gap-3 md:block">
            <Label class="text-sm text-muted-foreground">目标分</Label>
            <span class="tabular-nums text-xs text-muted-foreground md:mt-1 md:block">
              {autoQualityScoreLabel}
            </span>
          </div>
          <div class="flex h-8 min-w-0 items-center">
            <Slider
              type="single"
              bind:value={settings.autoQualityScore}
              min={50}
              max={95}
              step={1}
              disabled={busy}
              onValueCommit={commitAutoQualityScore}
            />
          </div>
        </div>
      {/if}

      {#if hasQualityFloor}
        <div
          class="grid gap-2 pt-1 md:grid-cols-[120px_minmax(0,1fr)]"
          class:opacity-40={!qualityEnabled || busy}
        >
          <div class="flex items-center justify-between gap-3 md:block">
            <Label class="text-sm text-muted-foreground">最低质量</Label>
            <span class="tabular-nums text-xs text-muted-foreground md:mt-1 md:block">
              {activeQualityFloorLabel}
            </span>
          </div>
          <div class="flex h-8 min-w-0 items-center">
            {#if settings.format === "jpeg"}
              <Slider
                type="single"
                bind:value={settings.jpegQualityFloor}
                min={0}
                max={100}
                step={1}
                disabled={!qualityEnabled || busy}
                onValueCommit={commitJpegQualityFloor}
              />
            {:else if settings.format === "webp"}
              <Slider
                type="single"
                bind:value={settings.webpQualityFloor}
                min={0}
                max={100}
                step={1}
                disabled={!qualityEnabled || busy}
                onValueCommit={commitWebpQualityFloor}
              />
            {:else if settings.format === "avif"}
              <Slider
                type="single"
                bind:value={settings.avifQualityFloor}
                min={0}
                max={100}
                step={1}
                disabled={!qualityEnabled || busy}
                onValueCommit={commitAvifQualityFloor}
              />
            {/if}
          </div>
        </div>
      {/if}
    </div>
  </div>
</section>
