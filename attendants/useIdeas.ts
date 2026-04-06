import { createSignal, createMemo } from 'solid-js'
import { useCastle } from '../cornerstone/useCastle'
import type { CastleResource } from '../cornerstone/types'
import type { ManifestEntry, IdeaPackage } from './types'

/**
 * Provider hook for Ideas (notes).
 * Replaces _shared/stores/tome.ts — all access goes through Castle.
 *
 * Must be called within a Castle-mounted component.
 */
export function useIdeas() {
  const castle = useCastle()

  // --- Notes list (auto-invalidated by idea.created/saved/deleted) ---
  const notes = castle.resource<ManifestEntry[]>('bard.list')

  // Sorted view: most recently modified first
  const sortedNotes = createMemo(() => {
    const raw = notes()
    if (!raw) return []
    return [...raw].sort((a, b) => {
      const ma = new Date(b.modified).getTime()
      const mb = new Date(a.modified).getTime()
      return ma - mb
    })
  })

  // --- Active note selection ---
  const [activeNoteId, setActiveNoteId] = createSignal<string | null>(null)

  // Active note package (reactive input, enabled gate)
  const activePackage = castle.resource<IdeaPackage | null>('bard.load', {
    input: () => ({ id: activeNoteId()! }),
    enabled: () => activeNoteId() !== null,
  })

  // --- Search ---
  const [searchQuery, setSearchQuery] = createSignal('')

  const filteredNotes = createMemo(() => {
    const q = searchQuery().toLowerCase().trim()
    const list = sortedNotes()
    if (!q) return list
    return list.filter(n => n.title.toLowerCase().includes(q))
  })

  // --- Actions ---

  function selectNote(id: string) {
    setActiveNoteId(id)
  }

  async function createNote(title?: string): Promise<string | null> {
    try {
      const result = await castle.court('bard.create', {
        type: 'text',
        title: title ?? 'Untitled',
        content: '',
      }) as { id?: string } | null

      await notes.refetch()

      const newId = result?.id ?? null
      if (newId) setActiveNoteId(newId)
      return newId
    } catch {
      return null
    }
  }

  async function deleteNote(id: string): Promise<void> {
    await castle.court('bard.delete', { id })

    // Clear selection if deleted note was active
    if (activeNoteId() === id) {
      setActiveNoteId(null)
    }

    await notes.refetch()
  }

  /** Optimistic signal mutation only — updates sidebar immediately, no persist. */
  function updateTitleDisplay(title: string) {
    const id = activeNoteId()
    if (!id) return
    notes.mutate((prev) => {
      if (!prev) return prev!
      return prev.map(n => n.id === id ? { ...n, title, modified: new Date().toISOString() } : n)
    })
  }

  /** Persist title to Chancellor (no signal mutation). */
  function persistTitle(title: string) {
    const id = activeNoteId()
    if (!id) return
    castle.court('bard.update_title', { id, title }).catch(() => {})
  }

  /** Update title display + persist immediately. Use outside collab. */
  function updateTitle(title: string) {
    updateTitleDisplay(title)
    persistTitle(title)
  }

  async function joinNote(ideaId: string): Promise<string | null> {
    try {
      const result = await castle.court('bard.create', {
        id: ideaId,
        type: 'text',
        title: 'Shared Note',
        content: '',
      }) as { id?: string } | null

      await notes.refetch()

      const newId = result?.id ?? null
      if (newId) setActiveNoteId(newId)
      return newId
    } catch {
      return null
    }
  }

  return {
    notes: sortedNotes,
    rawResource: notes,
    activeNoteId,
    activePackage: activePackage as CastleResource<IdeaPackage | null>,
    selectNote,
    createNote,
    deleteNote,
    updateTitle,
    updateTitleDisplay,
    persistTitle,
    joinNote,
    searchQuery,
    setSearchQuery,
    filteredNotes,
  }
}
