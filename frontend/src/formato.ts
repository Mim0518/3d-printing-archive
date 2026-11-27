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
  perfil: 'lime',
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

const pesos = new Intl.NumberFormat('es-MX', { style: 'currency', currency: 'MXN', maximumFractionDigits: 2 })
const centavos = new Intl.NumberFormat('es-MX', { style: 'currency', currency: 'MXN', maximumFractionDigits: 3 })

/** 437 → «$437.00»; con `fino`, 0.437 → «$0.437». */
export function dinero(x: number | null | undefined, fino = false): string {
  if (x == null) return ''
  return (fino ? centavos : pesos).format(x)
}

/** «2.2 s · base 19.8 s ×5» o «220 °C · cama 55 °C»: lo principal de un perfil. */
export function resumenPerfil(p: { capa_mm: number | null; parametros: Record<string, number | string> }): string {
  const x = p.parametros
  const num = (k: string) => (typeof x[k] === 'number' ? (x[k] as number) : null)
  const exp = num('exposicion_s')
  const base = num('exposicion_base_s')
  const boq = num('temp_boquilla_c')
  const cama = num('temp_cama_c')
  return [
    p.capa_mm ? `capa ${corto(p.capa_mm)} mm` : '',
    exp ? `${corto(exp, 3)} s` : '',
    base ? `base ${corto(base)} s${num('capas_base') ? ` ×${num('capas_base')}` : ''}` : '',
    boq ? `${boq} °C` : '',
    cama ? `cama ${cama} °C` : '',
    num('relleno_pct') != null ? `relleno ${num('relleno_pct')}%` : '',
  ]
    .filter(Boolean)
    .join(' · ')
}

/** «resina · 9024×5120 · 153.4×87×165 mm». */
export function resumenImpresora(i: {
  tecnologia: string
  res_x: number | null
  res_y: number | null
  boquilla_mm: number | null
  volumen_x_mm: number | null
  volumen_y_mm: number | null
  volumen_z_mm: number | null
}): string {
  return [
    i.tecnologia === 'fdm' ? 'FDM' : 'Resina',
    i.res_x && i.res_y ? `pantalla de ${i.res_x}×${i.res_y}` : '',
    i.boquilla_mm ? `boquilla de ${corto(i.boquilla_mm)} mm` : '',
    i.volumen_x_mm && i.volumen_y_mm && i.volumen_z_mm
      ? `${corto(i.volumen_x_mm)}×${corto(i.volumen_y_mm)}×${corto(i.volumen_z_mm)} mm`
      : '',
  ]
    .filter(Boolean)
    .join(' · ')
}
