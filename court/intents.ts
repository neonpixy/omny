import type { Intent } from '../cornerstone/types'

export const Intents: Record<string, Intent> = {
  'share-content': {
    description: 'Share content from one program to another',
    payload: { type: 'string', title: 'string', body: 'any', origin: 'string' },
  },
  'open-program': {
    description: 'Navigate to another program with optional context',
    payload: { slug: 'string', context: 'any?' },
  },
  'edit-content': {
    description: 'Open content in its native editor',
    payload: { contentId: 'string', contentType: 'string' },
  },
  'pick-item': {
    description: 'Ask user to select an item from another program',
    payload: { itemType: 'string', filter: 'any?' },
    returns: { itemId: 'string', preview: 'any' },
  },
  'request-identity': {
    description: 'Ask user to verify identity for an action',
    payload: { reason: 'string', level: 'string' },
  },
}

export function isValidIntent(name: string): boolean {
  return name in Intents
}
