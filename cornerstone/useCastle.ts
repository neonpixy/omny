import { useContext } from 'solid-js'
import { CastleContext } from './context'
import type { CastleRuntime } from './types'
import type { CourtierMap, TypedCastleRuntime } from '../orb/generated/registry'

/**
 * The one hook programs use to access Castle.
 * Must be called within a Castle-mounted program.
 *
 * Without a type parameter, returns the base CastleRuntime with string-based court().
 * With a type parameter, returns a TypedCastleRuntime with typed courtier access:
 *
 *   const castle = useCastle<['chamberlain', 'bard']>()
 *   castle.chamberlain.state()   // typed, autocomplete works
 *   castle.bard.list()           // typed
 *   castle.court('any.op', {})   // string-based still works
 */
export function useCastle<C extends (keyof CourtierMap)[] = []>(): C extends [] ? CastleRuntime : TypedCastleRuntime<C> {
  const ctx = useContext(CastleContext)
  if (!ctx) {
    throw new Error(
      'useCastle() must be used within a Castle-mounted program. ' +
      'Ensure your component is rendered inside a CastleContext.Provider.'
    )
  }
  return ctx as any
}
