import { useQuery } from '@tanstack/react-query'

import { api } from './api'
import type { Trabajo } from './tipos'

/** Sigue un trabajo en segundo plano (escaneo, mudanza…) hasta que termina. */
export function useTrabajo(id: number | null | undefined) {
  return useQuery({
    queryKey: ['trabajo', id],
    queryFn: () => api<Trabajo>(`/trabajos/${id}`),
    enabled: id != null,
    refetchInterval: (q) => (!q.state.data || q.state.data.estado === 'en_curso' ? 1000 : false),
  })
}
