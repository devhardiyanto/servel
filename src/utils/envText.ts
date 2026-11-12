/**
 * Teks environment per project ⇄ objek. Satu variabel per baris, `KEY=VALUE`.
 * Baris kosong dan baris diawali `#` diabaikan. Bentuk bash (`export KEY='v'`)
 * diterima supaya baris yang disalin dari terminal langsung jalan.
 * Aturan nama sama dengan `validate_env` di src-tauri/src/sites/process.rs.
 */
export interface EnvParse {
  env: Record<string, string>
  error: string | null
}

function unquote(v: string): string {
  if (v.length >= 2) {
    const q = v[0]
    if ((q === '"' || q === "'") && v[v.length - 1] === q) return v.slice(1, -1)
  }
  return v
}

export function parseEnvText(text: string): EnvParse {
  const env: Record<string, string> = {}
  const lines = text.split(/\r?\n/)
  for (let i = 0; i < lines.length; i++) {
    let line = lines[i].trim()
    if (line === '' || line.startsWith('#')) continue
    if (line.startsWith('export ')) line = line.slice(7).trim()
    const eq = line.indexOf('=')
    if (eq <= 0) return { env, error: `Line ${i + 1}: use KEY=VALUE.` }
    const key = line.slice(0, eq).trim()
    if (!/^[^\s=]+$/.test(key)) return { env, error: `Line ${i + 1}: "${key}" is not a valid variable name.` }
    if (key.toUpperCase() === 'PORT') return { env, error: `Line ${i + 1}: set the port in the Port field, not here.` }
    env[key] = unquote(line.slice(eq + 1).trim())
  }
  return { env, error: null }
}

export function envToText(env: Record<string, string> | undefined): string {
  return Object.entries(env ?? {})
    .map(([k, v]) => `${k}=${v}`)
    .join('\n')
}
