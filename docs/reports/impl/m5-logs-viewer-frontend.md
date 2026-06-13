# m5-logs-viewer-frontend

**Task:** Phase 4 Task 5 — Logs Viewer view (M5 UI)

## File yang diubah / dibuat

| File | Aksi |
|------|------|
| `src/views/LogsViewer.vue` | **Baru** — fullscreen log viewer |
| `src/views/Logs.vue` | Ubah — thin wrapper, delegasi ke `LogsViewer.vue` |
| `src/views/Dashboard.vue` | Ubah — inject `SetViewKey`, tambah `log-wrap__bar` dengan tombol "expand" ke view logs |

## Dependency plugin

- `@tauri-apps/plugin-dialog` — sudah ada di `package.json`, dipakai untuk `save()` dialog.
- `@tauri-apps/plugin-fs` — **belum diinstall**. Export Logs menggunakan fallback `Blob + URL.createObjectURL + <a download>` agar tidak memerlukan plugin tambahan. Jika ingin save ke path yang dipilih user secara native, install `@tauri-apps/plugin-fs` dan ganti `triggerDownload()` dengan `writeTextFile(filePath, content)`.

## Deviasi dari narasi Screen 4

1. **Tombol back** ditambahkan di sub-header kiri (arrow `←` → kembali ke dashboard). Narasi tidak menyebut ini tapi diperlukan karena tidak ada sidebar navigation global.
2. **Export via blob download** bukan native file dialog write — Tauri `save()` dipanggil untuk menampilkan dialog (UX konfirmasi path), tetapi karena `plugin-fs` belum ada, konten tetap di-download via browser blob. Jika user memilih cancel di dialog, `triggerDownload` tetap dipanggil (minor deviation — bisa diubah ke no-op kalau user cancel, tapi tidak kritis).
3. **Color mapping** diperluas: MYSQL = orange, REDIS = red, MAILPIT = purple (sesuai Screen 4 narasi), PGSQL di-map ke `log-tag--docker` (blue) karena tidak ada token khusus.
4. `navigation.ts` dan `App.vue` **tidak diubah** — `'logs'` sudah ada dari implementasi sebelumnya.

## Acceptance checklist

- [ ] `npm run build` PASS (vue-tsc + vite) — *belum bisa diverifikasi, izin shell ditolak*
- [x] Filter tab tersedia: All / MySQL / PostgreSQL / Redis / phpvm / fnm
- [x] Auto-scroll toggle state lokal default `true`, watch `lines.length`
- [x] Clear button memanggil `useLogs().clear()`
- [x] Export button: dialog + fallback blob download
- [x] Bottom bar: `{filteredCount} lines · {errorCount} errors`
- [x] Navigasi: tombol back ke dashboard, tombol expand di Dashboard log-wrap
