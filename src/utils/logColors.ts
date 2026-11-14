// Single source of truth for log source colours (Logs view and LogTail).
// Values mirror the --src-* tokens in assets/styles/tokens.css.

export interface BadgeStyle {
  color: string
  border: string
  background: string
}

export const SOURCE_COLORS: Record<string, string> = {
  SERVEL: '#7cc7e8',
  ENV: '#7cc7e8',
  DOCKER: '#8fb4f0',
  MYSQL: '#f2a97a',
  POSTGRES: '#8fb4f0',
  PGSQL: '#8fb4f0',
  REDIS: '#f19a9a',
  MAILPIT: '#c9a3f5',
  PHPVM: '#a3a8f0',
  PHP: '#a3a8f0',
  FNM: '#7fd6a4',
  NODE: '#7fd6a4',
  RABBITMQ: '#f2a97a',
  MONGODB: '#6cc9a1',
  MINIO: '#e8b45e',
  GOTENBERG: '#b9a4f2',
  MISE: '#a3a8f0',
  UP: '#7fd6a4',
  DOWN: '#f0c674',
}

const ERROR_COLOR = '#f19a9a'
const WARN_COLOR = '#f0c674'
const FALLBACK_COLOR = '#a1a1aa'

function tint(c: string): BadgeStyle {
  return { color: c, border: c + '33', background: c + '0d' }
}

export function badgeStyle(src?: string, level?: 'warn' | 'error'): BadgeStyle {
  if (level === 'error') return tint(ERROR_COLOR)
  if (level === 'warn') return tint(WARN_COLOR)
  return tint((src && SOURCE_COLORS[src.toUpperCase()]) || FALLBACK_COLOR)
}
