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
  renderizable: boolean
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
    placas?: number
    nuevas?: number
    leidos?: number
    sugerencias_nuevas?: number
  } | null
  creado: number
  terminado: number | null
  progreso: number | null
}

// ---------- placas ----------

export type EstadoPlaca = 'sin_probar' | 'probada' | 'fallida'
export type EstadoLiga = 'sugerido' | 'confirmado' | 'rechazado'

export interface DatosArchivoPlaca {
  ext: string
  rol: 'escena' | 'laminado' | 'autoguardado'
  formato: string | null
  maquina: string | null
  res_x: number | null
  res_y: number | null
  capa_mm: number | null
  exposicion_s: number | null
  exposicion_base_s: number | null
  capas_base: number | null
  capas: number | null
  alto_mm: number | null
  tiempo_s: number | null
  volumen_ml: number | null
  peso_g: number | null
  n_placas: number | null
  error: string | null
}

export interface ModeloDePlaca {
  id: number
  nombre: string
  faccion: string
  juego: string
  estudio: string
  carpeta: string
  estado: EstadoLiga
  origen: 'carpeta' | 'nombre' | 'manual'
  cantidad: number | null
}

export interface PlacaFila {
  id: number
  nombre: string
  carpeta: string
  origen: 'detectada' | 'manual'
  estado: EstadoPlaca
  favorita: boolean
  sin_modelo: boolean
  notas: string
  n_archivos: number
  n_presentes: number
  portada: { id: number; clave: string } | null
  formatos: string[]
  autoguardados: number
  laminados: DatosArchivoPlaca[]
  n_placas: number | null
  modelos: ModeloDePlaca[]
}

export interface ListaPlacas {
  total: number
  pagina: number
  por_pagina: number
  placas: PlacaFila[]
}

export interface ArchivoDePlaca {
  id: number
  ruta: string
  nombre: string
  ext: string
  tipo: string
  bytes: number
  mtime_ms: number
  estado: 'presente' | 'falta'
  rol: 'escena' | 'laminado' | 'autoguardado'
  auto: boolean
  modelo_id: number | null
  miniatura: string | null
  datos: DatosArchivoPlaca | null
}

export interface PlacaFicha extends Omit<PlacaFila, 'formatos' | 'autoguardados' | 'laminados' | 'n_placas'> {
  portada_elegida: number | null
  creada: number
  actualizada: number
  archivos: ArchivoDePlaca[]
  gemela: { id: number; ruta: string; miniatura: string | null } | null
}

export interface ResumenPlacas {
  total: number
  confirmadas: number
  sin_modelo: number
  por_confirmar: number
  sin_sugerencia: number
  sugerencias_por_carpeta: number
  con_laminado: number
  favoritas: number
  probadas: number
  fallidas: number
  vistas_pendientes: number
  en_curso: { id: number; tipo: string } | null
}
