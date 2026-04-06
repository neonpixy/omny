import { createContext } from 'solid-js'
import type { CastleRuntime } from './types'

export const CastleContext = createContext<CastleRuntime>()
