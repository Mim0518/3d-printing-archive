import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'

import { ApiError, api, type Usuario } from './api'

const CLAVE = ['sesion', 'yo'] as const

/** El usuario con sesión, o `null` si no hay (401). */
export function useYo() {
  return useQuery({
    queryKey: CLAVE,
    queryFn: async () => {
      try {
        return await api<Usuario>('/auth/yo')
      } catch (e) {
        if (e instanceof ApiError && e.status === 401) return null
        throw e
      }
    },
    retry: false,
    staleTime: 5 * 60 * 1000,
  })
}

export function useLogin() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: (datos: { usuario: string; contrasena: string }) =>
      api<Usuario>('/auth/login', { method: 'POST', body: JSON.stringify(datos) }),
    onSuccess: (u) => qc.setQueryData(CLAVE, u),
  })
}

export function useLogout() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: () => api<{ ok: boolean }>('/auth/logout', { method: 'POST' }),
    onSettled: () => {
      qc.setQueryData(CLAVE, null)
      qc.removeQueries({ predicate: (q) => q.queryKey[0] !== 'sesion' })
    },
  })
}
