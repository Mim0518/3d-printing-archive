const numero = new Intl.NumberFormat('es-MX')

export function n(x: number): string {
  return numero.format(x)
}

/** 1536 → «1.5 KB» (potencias de 1000, como los fabricantes de discos). */
export function tamano(bytes: number): string {
  if (bytes < 1000) return `${bytes} B`
  const u = ['KB', 'MB', 'GB', 'TB']
  let v = bytes
  let i = -1
  while (v >= 1000 && i < u.length - 1) {
    v /= 1000
    i++
  }
  return `${v >= 100 ? Math.round(v) : v.toFixed(1)} ${u[i]}`
}

const fecha = new Intl.DateTimeFormat('es-MX', { dateStyle: 'medium', timeStyle: 'short' })

export function cuando(segundosUnix: number | null | undefined): string {
  if (!segundosUnix) return 'nunca'
  return fecha.format(new Date(segundosUnix * 1000))
}

/** La misma regla que la columna J de la antigua hoja (y que el servidor). */
export function rutaNueva(juego: string, faccion: string, nombre: string, estudio: string, sub: string): string {
  let p = juego === 'D&D' ? 'DND' : `Wargames/${juego}`
  if (faccion) p += `/${faccion}`
  if (nombre) p += `/${nombre}${estudio ? ` (${estudio})` : ''}`
  if (sub) p += `/${sub}`
  return p
}

/** Nombre para mostrar: las filas de facción completa (OPR, Game Aids…) no tienen Modelo. */
export function titulo(m: { nombre: string; faccion: string; carpeta: string }): string {
  return m.nombre || m.faccion || m.carpeta.split('/').pop() || '(sin nombre)'
}

export const COLOR_TIPO: Record<string, string> = {
  malla: 'violet',
  escena: 'cyan',
  laminado: 'teal',
  imagen: 'pink',
  comprimido: 'orange',
  documento: 'gray',
  video: 'grape',
  otro: 'dark',
}

/** 18916 → «5 h 15 min». */
export function duracion(segundos: number | null | undefined): string {
  if (!segundos) return ''
  const h = Math.floor(segundos / 3600)
  const m = Math.round((segundos % 3600) / 60)
  if (h === 0) return `${m} min`
  return m ? `${h} h ${m} min` : `${h} h`
}

/** La impresora de un laminado: su nombre si lo trae, o la resolución de la pantalla. */
export function maquina(d: { maquina: string | null; res_x: number | null; res_y: number | null }): string {
  if (d.maquina && d.maquina !== 'Custom') return d.maquina
  if (d.res_x && d.res_y) return `${d.res_x}×${d.res_y}`
  return d.maquina ?? ''
}

/** Número corto: 0.05 → «0.05», 42.2981 → «42.3». */
export function corto(x: number | null | undefined, decimales = 1): string {
  if (x == null) return ''
  return x < 1 ? String(Number(x.toFixed(3))) : String(Number(x.toFixed(decimales)))
}
