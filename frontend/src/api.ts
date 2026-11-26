// Cliente mínimo de la API (/api/v1). La sesión viaja en una cookie HttpOnly que pone el servidor.

export class ApiError extends Error {
  readonly status: number

  constructor(status: number, mensaje: string) {
    super(mensaje)
    this.status = status
  }
}

export async function api<T>(ruta: string, opciones: RequestInit = {}): Promise<T> {
  const res = await fetch(`/api/v1${ruta}`, {
    credentials: 'same-origin',
    ...opciones,
    headers: {
      Accept: 'application/json',
      ...(opciones.body ? { 'Content-Type': 'application/json' } : {}),
      ...opciones.headers,
    },
  })
  const texto = await res.text()
  let datos: unknown = null
  try {
    datos = texto ? JSON.parse(texto) : null
  } catch {
    datos = null
  }
  if (!res.ok) {
    const mensaje =
      datos && typeof datos === 'object' && 'error' in datos && typeof datos.error === 'string'
        ? datos.error
        : `Error ${res.status}`
    throw new ApiError(res.status, mensaje)
  }
  return datos as T
}

export interface Usuario {
  id: number
  nombre: string
}

export interface Salud {
  estado: string
  version: string
}
