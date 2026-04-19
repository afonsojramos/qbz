import { writable } from 'svelte/store';
import type { AddToMixtapeItem } from '$lib/components/AddToMixtapeModal.svelte';

interface State {
  open: boolean;
  item: AddToMixtapeItem | null;
}

export const addToMixtapeModal = writable<State>({ open: false, item: null });

export function openAddToMixtape(item: AddToMixtapeItem): void {
  addToMixtapeModal.set({ open: true, item });
}

export function closeAddToMixtape(): void {
  addToMixtapeModal.set({ open: false, item: null });
}
