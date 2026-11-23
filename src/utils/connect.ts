// Info koneksi per service: host, port, kredensial, URL, dan baris
// .env siap tempel. Nilai diambil dari definisi service (services.json),
// jadi port/kredensial yang diubah di sana ikut berubah di sini.
import type { ServiceDef } from '@/types/service'

export interface ConnectField {
  label: string
  value: string
}

export interface ConnectInfo {
  fields: ConnectField[]
  /** URL koneksi utama (dipakai aplikasi/driver). */
  url: string
  /** Baris .env (format Laravel) untuk disalin. */
  env: string
  /** UI web bawaan service, bila ada. */
  webUrl?: string
}

const HOST = '127.0.0.1'

function hostPort(def: ServiceDef, container: string): string {
  return def.ports.find((p) => p.container === container)?.host ?? def.ports[0]?.host ?? ''
}

function lines(entries: Array<[string, string]>): string {
  return entries.map(([k, v]) => `${k}=${v}`).join('\n')
}

export function connectionInfo(def: ServiceDef): ConnectInfo {
  const env = def.environment ?? {}
  switch (def.id) {
    case 'mysql': {
      const port = hostPort(def, '3306')
      const db = env.MYSQL_DATABASE ?? ''
      const user = 'root'
      const password = env.MYSQL_ROOT_PASSWORD ?? ''
      return {
        fields: [
          { label: 'Host', value: HOST },
          { label: 'Port', value: port },
          { label: 'Database', value: db },
          { label: 'User', value: user },
          { label: 'Password', value: password || '(empty)' },
        ],
        url: `mysql://${user}${password ? `:${password}` : ''}@${HOST}:${port}/${db}`,
        env: lines([
          ['DB_CONNECTION', 'mysql'],
          ['DB_HOST', HOST],
          ['DB_PORT', port],
          ['DB_DATABASE', db],
          ['DB_USERNAME', user],
          ['DB_PASSWORD', password],
        ]),
      }
    }
    case 'postgres': {
      const port = hostPort(def, '5432')
      const db = env.POSTGRES_DB ?? 'postgres'
      const user = env.POSTGRES_USER ?? 'postgres'
      const password = env.POSTGRES_PASSWORD ?? ''
      return {
        fields: [
          { label: 'Host', value: HOST },
          { label: 'Port', value: port },
          { label: 'Database', value: db },
          { label: 'User', value: user },
          { label: 'Password', value: password || '(not required)' },
        ],
        url: `postgresql://${user}${password ? `:${password}` : ''}@${HOST}:${port}/${db}`,
        env: lines([
          ['DB_CONNECTION', 'pgsql'],
          ['DB_HOST', HOST],
          ['DB_PORT', port],
          ['DB_DATABASE', db],
          ['DB_USERNAME', user],
          ['DB_PASSWORD', password],
        ]),
      }
    }
    case 'redis': {
      const port = hostPort(def, '6379')
      return {
        fields: [
          { label: 'Host', value: HOST },
          { label: 'Port', value: port },
        ],
        url: `redis://${HOST}:${port}`,
        env: lines([
          ['REDIS_HOST', HOST],
          ['REDIS_PORT', port],
          ['REDIS_PASSWORD', 'null'],
        ]),
      }
    }
    case 'rabbitmq': {
      const port = hostPort(def, '5672')
      const user = env.RABBITMQ_DEFAULT_USER ?? 'guest'
      const password = env.RABBITMQ_DEFAULT_PASS ?? 'guest'
      return {
        fields: [
          { label: 'Host', value: HOST },
          { label: 'Port', value: port },
          { label: 'User', value: user },
          { label: 'Password', value: password },
        ],
        url: `amqp://${user}:${password}@${HOST}:${port}`,
        env: lines([
          ['RABBITMQ_HOST', HOST],
          ['RABBITMQ_PORT', port],
          ['RABBITMQ_USER', user],
          ['RABBITMQ_PASSWORD', password],
        ]),
        webUrl: `http://localhost:${hostPort(def, '15672')}`,
      }
    }
    case 'mongodb': {
      const port = hostPort(def, '27017')
      return {
        fields: [
          { label: 'Host', value: HOST },
          { label: 'Port', value: port },
        ],
        url: `mongodb://${HOST}:${port}`,
        env: lines([['MONGODB_URI', `mongodb://${HOST}:${port}`]]),
      }
    }
    case 'minio': {
      const port = hostPort(def, '9000')
      const user = env.MINIO_ROOT_USER ?? 'minioadmin'
      const password = env.MINIO_ROOT_PASSWORD ?? 'minioadmin'
      return {
        fields: [
          { label: 'Endpoint', value: `http://${HOST}:${port}` },
          { label: 'Access key', value: user },
          { label: 'Secret key', value: password },
        ],
        url: `http://${HOST}:${port}`,
        env: lines([
          ['AWS_ACCESS_KEY_ID', user],
          ['AWS_SECRET_ACCESS_KEY', password],
          ['AWS_DEFAULT_REGION', 'us-east-1'],
          ['AWS_ENDPOINT', `http://${HOST}:${port}`],
          ['AWS_USE_PATH_STYLE_ENDPOINT', 'true'],
        ]),
        webUrl: `http://localhost:${hostPort(def, '9001')}`,
      }
    }
    case 'mailpit': {
      const port = hostPort(def, '1025')
      return {
        fields: [
          { label: 'SMTP host', value: HOST },
          { label: 'SMTP port', value: port },
        ],
        url: `smtp://${HOST}:${port}`,
        env: lines([
          ['MAIL_MAILER', 'smtp'],
          ['MAIL_HOST', HOST],
          ['MAIL_PORT', port],
          ['MAIL_USERNAME', 'null'],
          ['MAIL_PASSWORD', 'null'],
          ['MAIL_ENCRYPTION', 'null'],
        ]),
        webUrl: `http://localhost:${hostPort(def, '8025')}`,
      }
    }
    case 'gotenberg': {
      const port = hostPort(def, '3000')
      return {
        fields: [{ label: 'URL', value: `http://${HOST}:${port}` }],
        url: `http://${HOST}:${port}`,
        env: lines([['GOTENBERG_URL', `http://${HOST}:${port}`]]),
      }
    }
    default: {
      const port = def.ports[0]?.host ?? ''
      const env = def.custom && def.connectEnv ? def.connectEnv.split(/\r?\n/).map((l) => l.split('{port}').join(port)).filter((l) => l.trim()).join('\n') : ''
      const url = env.match(/=(https?:\/\/\S+)/)?.[1] ?? `${HOST}:${port}`
      return {
        fields: [
          { label: 'Host', value: HOST },
          { label: 'Port', value: port },
          ...(def.custom ? [{ label: 'Network name', value: `${def.containerName}:${def.ports[0]?.container ?? port}` }] : []),
        ],
        url,
        env,
      }
    }
  }
}

/** Format baris .env di panel Connect. */
export type EnvFormat = 'laravel' | 'url' | 'go'

const URL_KEY: Record<string, string> = {
  mysql: 'DATABASE_URL',
  postgres: 'DATABASE_URL',
  redis: 'REDIS_URL',
  rabbitmq: 'RABBITMQ_URL',
  mongodb: 'MONGODB_URI',
  minio: 'S3_ENDPOINT',
  mailpit: 'SMTP_URL',
  gotenberg: 'GOTENBERG_URL',
}

/**
 * Baris .env dalam format pilihan.
 * - laravel: nama variabel bawaan Laravel (DB_HOST, REDIS_HOST, ...).
 * - url: satu URL koneksi (DATABASE_URL, REDIS_URL, ...).
 * - go: bentuk yang dipakai driver Go umum (DSN go-sql-driver/mysql,
 *   alamat go-redis, URL pgx dengan sslmode=disable).
 */
export function envFor(def: ServiceDef, format: EnvFormat): string {
  const info = connectionInfo(def)
  if (format === 'laravel') return info.env
  const key = URL_KEY[def.id] ?? `${def.id.toUpperCase()}_URL`
  if (format === 'url') return info.url ? `${key}=${info.url}` : ''
  const field = (label: string) => info.fields.find((f) => f.label === label)?.value ?? ''
  switch (def.id) {
    case 'mysql': {
      const pw = def.environment?.MYSQL_ROOT_PASSWORD ?? ''
      return `DB_DSN=root${pw ? `:${pw}` : ''}@tcp(${HOST}:${field('Port')})/${field('Database')}?parseTime=true`
    }
    case 'postgres':
      return `DATABASE_URL=${info.url}?sslmode=disable`
    case 'redis':
      return `REDIS_ADDR=${HOST}:${field('Port')}`
    default:
      return info.url ? `${key}=${info.url}` : ''
  }
}
