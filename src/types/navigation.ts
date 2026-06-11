import type { InjectionKey } from 'vue'

export type ViewName = 'onboarding' | 'dashboard' | 'settings' | 'logs'

export type SetViewFn = (view: ViewName) => void

export const SetViewKey: InjectionKey<SetViewFn> = Symbol('setView')
