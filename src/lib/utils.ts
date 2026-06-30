import { clsx, type ClassValue } from "clsx";
import { twMerge } from "tailwind-merge";

export type { WithElementRef, WithoutChild, WithoutChildrenOrChild } from "bits-ui";

/** 合并 Tailwind class(shadcn-svelte 约定的 cn 助手)。 */
export function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs));
}
