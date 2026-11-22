import type { InjectionKey } from 'vue'

/** Navigasi kiri. `dashboard` diterima sebagai alias lama untuk `services`. */
export type ViewName = 'onboarding' | 'projects' | 'services' | 'runtimes' | 'logs' | 'settings'

export type SetViewFn = (view: ViewName | 'dashboard') => void

export const SetViewKey: InjectionKey<SetViewFn> = Symbol('setView')
