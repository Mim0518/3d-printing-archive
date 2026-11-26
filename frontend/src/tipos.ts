// Formas de las respuestas de la API (/api/v1).

export interface Resumen {
  modelos: { total: number; revisados: number }
  archivos: { total: number; bytes: number; sin_modelo: number; faltan: number }
  por_tipo: { tipo: string; archivos: number; bytes: number }[]
  por_juego: { juego: string; modelos: number; bytes: number; con_frio: number }[]
  fuera_del_catalogo: { carpeta: string; archivos: number; bytes: number }[]
  frio: { ubicacion: string; remoto: string; copias: number; bytes: number; ligadas: number }[]
  ultimo_escaneo: number | null
  escaneo_en_curso: number | null
}

export interface ModeloFila {
  id: number
  carpeta: string
  juego: string
  faccion: string
  nombre: string
  estudio: string
  subcarpeta: string
  confianza: string
  revisado: string
  n_archivos: number
  n_mallas: number
  bytes: number
  n_frio: number
  bytes_frio: number
  ruta_nueva: string
  pendiente: boolean
  n_avisos: number
  etiquetas: string[]
  portada: { id: number; clave: string } | null
}

export interface ListaModelos {
  total: number
  pagina: number
  por_pagina: number
  modelos: ModeloFila[]
}

export interface OpcionesFiltros {
  juegos: { nombre: string; modelos: number }[]
  facciones: { juego: string; nombre: string; modelos: number }[]
  estudios: { nombre: string; modelos: number }[]
  etiquetas: { nombre: string; tipo: string; modelos: number }[]
}

export interface ArchivoFila {
  id: number
  ruta: string
  nombre: string
  ext: string
  tipo: string
  bytes: number
  nlink: number
  stl_formato: string | null
  triangulos: number | null
  estado: 'presente' | 'falta'
  miniatura: string | null
}

export interface CopiaFrio {
  id: number
  ubicacion: string
  remoto: string
  ruta: string
  bytes: number
  md5: string
  ruta_original: string
  entradas: number
}

export interface Ficha extends ModeloFila {
  notas: string
  portada_elegida: number | null
  avisos: string[]
  archivos: ArchivoFila[]
  copias_frio: CopiaFrio[]
}

export interface Cambio {
  campo: string
  antes: string
  despues: string
  usuario: string
  fecha: number
}

export interface Etiqueta {
  id: number
  nombre: string
  tipo: string
  color: string
  modelos: number
}

export interface ModeloPlan {
  id: number
  nombre: string
  de: string
  a: string
  archivos: number
  bytes: number
}

export interface Plan {
  id: number
  modelos: ModeloPlan[]
  archivos: number
  bytes: number
  conflictos: string[]
  avisos: string[]
  se_quedan: number
  rutas_largas: number
}

export interface MudanzaFila {
  id: number
  estado: 'plan' | 'en_curso' | 'ejecutada' | 'con_errores' | 'deshecha' | 'descartada'
  usuario: string
  creada: number
  ejecutada: number | null
  deshecha: number | null
  modelos: number
  archivos: number
  bytes: number
  conflictos: number
}

export interface MudanzaDetalle {
  id: number
  estado: MudanzaFila['estado']
  usuario: string
  creada: number
  ejecutada: number | null
  deshecha: number | null
  deshacible: boolean
  plan: Plan
  archivos_por_estado: Record<string, number>
  errores: { archivo: string; mensaje: string }[]
  muestra: { de: string; a: string }[]
}

export interface ResumenMiniaturas {
  archivos: number
  con_miniatura: number
  errores: number
  pendientes: number
  en_curso: number | null
}

export interface Trabajo {
  id: number
  tipo: string
  estado: 'en_curso' | 'terminado' | 'fallo'
  detalle: string
  resultado: {
    archivos?: number
    nuevos?: number
    cambiados?: number
    iguales?: number
    faltan?: number
    movidos?: number
    carpetas_quitadas?: number
    candidatos?: number
    ya_estaban?: number
    generadas?: number
    fallidas?: number
    ejemplos_de_error?: string[]
    errores?: string[]
  } | null
  creado: number
  terminado: number | null
  progreso: number | null
}
