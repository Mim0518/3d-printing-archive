import { useQuery } from '@tanstack/react-query'

import { api } from './api'
import type { Etiqueta, OpcionesFiltros } from './tipos'

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
