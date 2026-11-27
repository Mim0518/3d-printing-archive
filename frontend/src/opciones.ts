import { useQuery, useQueryClient } from '@tanstack/react-query'

import { api } from './api'
import type { Campo, Etiqueta, Impresora, Material, OpcionesFiltros, Perfil } from './tipos'

export const REVISADO = [
  { value: '', label: 'Sin revisar' },
  { value: 'OK', label: 'OK' },
  { value: 'Corregido', label: 'Corregido' },
  { value: 'No mover', label: 'No mover' },
]

export function useOpciones() {
  const filtros = useQuery({
    queryKey: ['filtros'],
    queryFn: () => api<OpcionesFiltros>('/modelos/filtros'),
    staleTime: 60_000,
  })
  const etiquetas = useQuery({ queryKey: ['etiquetas'], queryFn: () => api<Etiqueta[]>('/etiquetas'), staleTime: 60_000 })
  return { filtros: filtros.data, etiquetas: etiquetas.data ?? [] }
}

export const ESTADO_MUDANZA: Record<string, { texto: string; color: string }> = {
  plan: { texto: 'plan', color: 'blue' },
  en_curso: { texto: 'en curso', color: 'violet' },
  ejecutada: { texto: 'ejecutada', color: 'green' },
  con_errores: { texto: 'con errores', color: 'red' },
  deshecha: { texto: 'deshecha', color: 'gray' },
  descartada: { texto: 'descartada', color: 'gray' },
}

/** Formatos que abre el visor 3D. */
export const VISIBLES_3D = ['stl', 'obj', '3mf', 'ply']

export const ESTADO_PLACA: Record<string, { texto: string; color: string }> = {
  sin_probar: { texto: 'sin probar', color: 'gray' },
  probada: { texto: 'probada', color: 'green' },
  fallida: { texto: 'fallida', color: 'red' },
}

export const ROL_ARCHIVO: Record<string, string> = {
  escena: 'escena',
  laminado: 'laminado',
  autoguardado: 'autoguardado',
}

/** Impresoras, materiales, perfiles y los campos de cada tecnología (son pocos: se piden enteros). */
export function useTaller() {
  const impresoras = useQuery({ queryKey: ['impresoras'], queryFn: () => api<Impresora[]>('/impresoras') })
  const materiales = useQuery({ queryKey: ['materiales'], queryFn: () => api<Material[]>('/materiales') })
  const perfiles = useQuery({ queryKey: ['perfiles'], queryFn: () => api<Perfil[]>('/perfiles') })
  const campos = useQuery({ queryKey: ['campos'], queryFn: () => api<Campo[]>('/perfiles/campos'), staleTime: Infinity })
  return {
    impresoras: impresoras.data ?? [],
    materiales: materiales.data ?? [],
    perfiles: perfiles.data ?? [],
    campos: campos.data ?? [],
    cargando: impresoras.isPending || materiales.isPending || perfiles.isPending,
    error: impresoras.error ?? materiales.error ?? perfiles.error,
  }
}

/** Lo que cambia algo del taller: se vuelven a pedir todas sus listas. */
export const CLAVES_TALLER = ['impresoras', 'materiales', 'perfiles', 'impresora', 'perfil', 'taller-resumen', 'importables']

export function nombreMaterial(m: { marca: string; nombre: string }): string {
  return `${m.marca} ${m.nombre}`.trim()
}

/** '' (campo vacío) → null; lo demás, número. */
export function aNumero(v: number | string | null | undefined): number | null {
  if (v === '' || v == null) return null
  const n = typeof v === 'number' ? v : Number(v)
  return Number.isFinite(n) ? n : null
}

/** Vuelve a pedir todo lo del taller (y las placas, que muestran impresora y perfil). */
export function useRefrescarTaller() {
  const qc = useQueryClient()
  return () => {
    for (const k of [...CLAVES_TALLER, 'placas', 'placa']) void qc.invalidateQueries({ queryKey: [k] })
  }
}
