<!-- SPDX-License-Identifier: Apache-2.0 -->
<script lang="ts">
  import { Images, Sun, Moon, Desktop, Info, Sparkle } from "phosphor-svelte";
  import { Button } from "$lib/components/ui/button";
  import LegalDialog from "$lib/components/LegalDialog.svelte";
  import { settings, engine, applyTheme, persistSettings } from "$lib/state.svelte";

  const themeIcon = $derived(
    settings.theme === "dark" ? Moon : settings.theme === "light" ? Sun : Desktop,
  );
  const themeLabel = $derived(
    settings.theme === "dark" ? "深色" : settings.theme === "light" ? "浅色" : "跟随系统",
  );
  let legalOpen = $state(false);

  function cycleTheme() {
    const order = ["light", "dark", "system"] as const;
    settings.theme = order[(order.indexOf(settings.theme) + 1) % order.length];
    applyTheme();
    persistSettings();
  }
  function toggleMotion() {
    settings.reduceMotion = !settings.reduceMotion;
    applyTheme();
    persistSettings();
  }

  const ThemeIcon = $derived(themeIcon);
</script>

<header class="flex items-center gap-3">
  <Images size={26} class="text-primary" weight="duotone" />
  <h1 class="text-lg font-bold">ImgConvert</h1>
  <span class="text-xs {engine.ok ? 'text-emerald-600' : 'text-destructive'}">
    {engine.text}
  </span>

  <div class="ml-auto flex items-center gap-1">
    <Button
      variant="ghost"
      size="icon"
      title="减弱动效:{settings.reduceMotion ? '开' : '关'}"
      class={settings.reduceMotion ? "text-primary" : "text-muted-foreground"}
      onclick={toggleMotion}
    >
      <Sparkle weight={settings.reduceMotion ? "regular" : "duotone"} />
    </Button>
    <Button variant="ghost" size="icon" title="主题:{themeLabel}" onclick={cycleTheme}>
      <ThemeIcon weight="duotone" />
    </Button>
    <Button variant="ghost" size="icon" title="开源许可" onclick={() => (legalOpen = true)}>
      <Info weight="duotone" />
    </Button>
  </div>
</header>

<LegalDialog bind:open={legalOpen} />
