import type { Pipeline } from '../cornerstone/types'

export const Pipelines: Record<string, Pipeline> = {
  'bard.create-and-store': {
    description: 'Create a new idea and persist to vault',
    requires: ['bard', 'castellan', 'clerk'],
    steps: [
      { id: 'idea', op: 'bard.package_new', input: { type_name: '$type', title: '$title' } },
      { id: 'key', op: 'castellan.content_key', input: { idea_id: '$idea.result.id' } },
      { id: 'write', op: 'clerk.write', input: { package: '$idea.result', key: '$key.result' } },
    ],
    returns: '$idea.result',
  },
  'content.share': {
    description: 'Load content and share via intent',
    requires: ['bard', 'share-content'],
    steps: [
      { id: 'load', op: 'bard.package_header', input: { idea_id: '$idea_id' } },
      { id: 'share', op: 'intent:share-content', input: { type: '$load.result.type', title: '$load.result.title', body: '$load.result', origin: '$caller' } },
    ],
  },
  'identity.verify': {
    description: 'Verify crown is unlocked before sensitive action',
    requires: ['chamberlain'],
    steps: [
      { id: 'state', op: 'chamberlain.soul_profile', input: {} },
      { id: 'check', op: 'chamberlain.keyring_is_unlocked', input: {} },
    ],
    returns: '$state.result',
  },
}

export function isValidPipeline(name: string): boolean {
  return name in Pipelines
}
