<!-- SPDX-License-Identifier: Apache-2.0 -->
<script lang="ts">
  import { FolderOpen, ArrowsClockwise, WarningCircle } from "phosphor-svelte";
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
    capabilities,
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
  import { cn } from "$lib/utils.js";

  let { variant = "bar" }: { variant?: "bar" | "panel" } = $props();
  let outputMessage = $state("");
  let activeSwitchHelp = $state<SwitchHelpKey | null>(null);
  const busy = $derived(ui.converting || ui.importing);
  const isPanel = $derived(variant === "panel");

  const switchHelp = {
    lossless: {
      label: "无损压缩",
      description: "优先保持像素不损失；仅在目标格式支持时可用，文件体积不一定最小。",
    },
    skipIfLarger: {
      label: "跳过变大输出",
      description: "候选输出不小于源文件时不写入，避免转换后文件反而变大。",
    },
    multiCandidate: {
      label: "多候选取最小",
      description: "尝试多个等价编码候选，并写入体积最小的结果，耗时会略有增加。",
    },
    autoQuality: {
      label: "自动质量",
      description: "JPEG/WebP 会搜索达到目标评分的最低质量，尽量减少体积。",
    },
    generationLossProtection: {
      label: "代际防护",
      description: "有损源再次输出有损格式时要求足够体积收益，减少反复压缩造成的劣化。",
    },
    resultCache: {
      label: "结果缓存",
      description: "源文件与设置未变时复用已有输出，适合重复处理同一批文件。",
    },
    preserveMetadata: {
      label: "保留元数据",
      description: "尽量保留 ICC 色彩配置和 EXIF 元数据；部分格式或字段可能无法写回。",
    },
    convertToSrgb: {
      label: "转为 sRGB",
      description: "使用嵌入 ICC 转换像素到 sRGB，并移除旧 ICC，适合统一网页和通用查看器显示。",
    },
  } as const;

  type SwitchHelpKey = keyof typeof switchHelp;

  const activeSwitchHelpContent = $derived(activeSwitchHelp ? switchHelp[activeSwitchHelp] : null);

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

  function toggleSwitchHelp(key: SwitchHelpKey) {
    activeSwitchHelp = activeSwitchHelp === key ? null : key;
  }

  function updateSetting<T extends boolean>(setter: (checked: T) => void) {
    return (checked: T) => {
      setter(checked);
      persistSettings();
    };
  }
</script>

{#snippet settingSwitch(
  key: SwitchHelpKey,
  label: string,
  checked: boolean,
  disabled: boolean,
  onChange: (checked: boolean) => void,
  muted = false,
)}
  <div
    class={cn(
      "grid min-h-[4.25rem] grid-cols-[minmax(0,1fr)_auto] items-start gap-x-2 gap-y-1 rounded-md border bg-background/70 px-2.5 py-2 transition-colors",
      activeSwitchHelp === key
        ? "border-primary/40 bg-primary/5"
        : "border-border/80 hover:border-foreground/20 hover:bg-muted/45",
      muted && "opacity-45",
    )}
  >
    <span class="min-w-0 text-pretty text-sm font-medium leading-5">{label}</span>
    <Button
      variant="ghost"
      size="icon"
      class={cn(
        "size-7 rounded-md text-muted-foreground hover:text-foreground",
        activeSwitchHelp === key && "bg-background text-foreground",
      )}
      aria-label={`说明：${label}`}
      aria-pressed={activeSwitchHelp === key}
      onclick={() => toggleSwitchHelp(key)}
    >
      <WarningCircle size={15} weight={activeSwitchHelp === key ? "fill" : "duotone"} />
    </Button>
    <div class="col-span-2 flex items-center justify-between gap-3">
      <span class="text-[11px] leading-none text-muted-foreground">
        {checked ? "已开启" : "未开启"}
      </span>
      <Switch size="sm" aria-label={label} {checked} {disabled} onCheckedChange={onChange} />
    </div>
  </div>
{/snippet}

<section class={cn("rounded-lg border bg-card p-4", isPanel && "h-fit")}>
  <div
    class={cn(
      "grid gap-4",
      isPanel ? "grid-cols-1" : "lg:grid-cols-[220px_minmax(220px,1fr)_180px_180px_220px]",
    )}
  >
    {#if isPanel}
      <div class="flex items-center justify-between gap-3 border-b pb-3">
        <div class="min-w-0">
          <h2 class="text-sm font-semibold">转换设置</h2>
          <p class="mt-1 truncate text-xs text-muted-foreground">
            {settings.format.toUpperCase()} · {qualityTitle}
            {qualityLabel}
          </p>
        </div>
        <Button
          variant="ghost"
          size="sm"
          onclick={resetItemFormats}
          disabled={busy || !queue.length}
          class="shrink-0"
        >
          <ArrowsClockwise weight="duotone" />
          跟随全局
        </Button>
      </div>
    {/if}

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

    <div class={cn("flex min-w-0 flex-col gap-2", !qualityEnabled || busy ? "opacity-40" : "")}>
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

    <div class={cn("flex min-w-0 flex-col gap-2", busy ? "opacity-40" : "")}>
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

    <div class={cn("min-w-0", isPanel ? "border-t pt-3" : "lg:col-span-3")}>
      {#if activeSwitchHelpContent}
        <div
          class="mb-2 rounded-md border border-primary/15 bg-muted/55 px-3 py-2 text-xs leading-5 text-muted-foreground"
        >
          <span class="font-medium text-foreground">{activeSwitchHelpContent.label}</span>
          <span>：{activeSwitchHelpContent.description}</span>
        </div>
      {/if}

      <div class="grid grid-cols-1 gap-2 sm:grid-cols-2">
        {@render settingSwitch(
          "lossless",
          "无损压缩",
          settings.lossless,
          !canLossless || busy,
          updateSetting((checked) => {
            settings.lossless = checked;
          }),
          !canLossless || busy,
        )}
        {@render settingSwitch(
          "skipIfLarger",
          "跳过变大输出",
          settings.skipIfLarger,
          busy,
          updateSetting((checked) => {
            settings.skipIfLarger = checked;
          }),
          busy,
        )}
        {@render settingSwitch(
          "multiCandidate",
          "多候选取最小",
          settings.multiCandidate,
          busy,
          updateSetting((checked) => {
            settings.multiCandidate = checked;
          }),
          busy,
        )}
        {@render settingSwitch(
          "autoQuality",
          "自动质量",
          settings.autoQuality && autoQualitySupported,
          !autoQualitySupported || busy,
          updateSetting((checked) => {
            settings.autoQuality = checked;
          }),
          !autoQualitySupported || busy,
        )}
        {@render settingSwitch(
          "generationLossProtection",
          "代际防护",
          settings.generationLossProtection,
          busy,
          updateSetting((checked) => {
            settings.generationLossProtection = checked;
          }),
          busy,
        )}
        {@render settingSwitch(
          "resultCache",
          "结果缓存",
          settings.resultCache,
          busy,
          updateSetting((checked) => {
            settings.resultCache = checked;
          }),
          busy,
        )}
        {@render settingSwitch(
          "preserveMetadata",
          "保留元数据",
          settings.preserveMetadata,
          busy,
          updateSetting((checked) => {
            settings.preserveMetadata = checked;
          }),
          busy,
        )}
        {@render settingSwitch(
          "convertToSrgb",
          "转为 sRGB",
          settings.colorManagementPolicy === "convertToSrgb",
          busy || !capabilities.colorPipeline.iccTransform,
          updateSetting((checked) => {
            settings.colorManagementPolicy = checked ? "convertToSrgb" : "preserve";
          }),
          busy || !capabilities.colorPipeline.iccTransform,
        )}
      </div>
    </div>

    {#if !isPanel}
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
    {/if}

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

    <div class={cn("flex min-w-0 flex-col gap-2 border-t pt-3", isPanel ? "" : "lg:col-span-5")}>
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
