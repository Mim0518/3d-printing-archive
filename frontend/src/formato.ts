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
