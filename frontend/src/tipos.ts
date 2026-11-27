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
  /** Reconocidos por la pantalla del laminado y por su capa y exposición. */
  impresora?: { id: number; nombre: string } | null
  perfil?: { id: number; nombre: string } | null
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
  impresora_id: number | null
  material_id: number | null
  perfil_id: number | null
  /** Elegida a mano y las que se reconocen por los laminados. */
  impresoras: Ref[]
  perfil: Ref | null
  material: Ref | null
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

// ---------- fase 5: impresoras, materiales y perfiles ----------

export type Tecnologia = 'resina' | 'fdm'

/** Algo ligado y por qué: elegido a mano, reconocido por el laminado o tomado del perfil. */
export interface Ref {
  id: number
  nombre: string
  por: 'elegida' | 'elegido' | 'laminado' | 'perfil'
}

export interface Impresora {
  id: number
  nombre: string
  tecnologia: Tecnologia
  marca: string
  modelo: string
  volumen_x_mm: number | null
  volumen_y_mm: number | null
  volumen_z_mm: number | null
  boquilla_mm: number | null
  res_x: number | null
  res_y: number | null
  pixel_um: number | null
  potencia_w: number | null
  carpeta: string
  perfil_id: number | null
  perfil: string | null
  en_uso: boolean
  notas: string
  origen: string | null
  n_perfiles: number
  n_placas: number
  n_laminados: number
}

export interface DocumentoImpresora {
  id: number
  ruta: string
  carpeta: string
  nombre: string
  ext: string
  tipo: string
  bytes: number
  mtime_ms: number
  miniatura: string | null
}

export interface ImpresoraFicha extends Impresora {
  perfiles: Perfil[]
  documentos: DocumentoImpresora[]
}

export interface Material {
  id: number
  tipo: 'resina' | 'filamento'
  marca: string
  nombre: string
  subtipo: string
  color: string
  color_hex: string
  presentacion: number | null
  precio: number | null
  precio_unidad: number | null
  densidad: number | null
  diametro_mm: number | null
  queda: number | null
  en_uso: boolean
  notas: string
  origen: string | null
  n_perfiles: number
  n_placas: number
}

export type Parametros = Record<string, number | string>

export interface Perfil {
  id: number
  nombre: string
  impresora_id: number
  impresora: string
  tecnologia: Tecnologia
  material_id: number | null
  material: string | null
  capa_mm: number | null
  parametros: Parametros
  version: number
  en_uso: boolean
  predeterminado: boolean
  notas: string
  origen: 'manual' | 'importado'
  n_archivos: number
  creado: number
  actualizado: number
}

export interface VersionPerfil {
  version: number
  material: string | null
  capa_mm: number | null
  parametros: Parametros
  desde: number
  hasta: number
  usuario: string
}

export interface ArchivoPerfil {
  id: number
  ruta: string
  nombre: string
  ext: string
  tipo: string
  bytes: number
  mtime_ms: number
  estado: 'presente' | 'falta'
}

export interface PerfilFicha extends Perfil {
  archivos: ArchivoPerfil[]
  versiones: VersionPerfil[]
  placas: { id: number; nombre: string; carpeta: string; por: 'elegido' | 'laminado' }[]
}

export interface Campo {
  clave: string
  etiqueta: string
  unidad: string
  tecnologia: Tecnologia
  tipo: 'numero' | 'entero' | 'texto'
  min: number
  max: number
}

export interface Propuesta {
  clave: string
  clase: 'impresora' | 'material' | 'perfil'
  nombre: string
  detalle: string
  datos: Record<string, unknown> & { parametros?: Parametros; capa_mm?: number }
  impresora: string | null
  material: string | null
  archivos: number[]
  fuentes: string[]
  predeterminado: boolean
  marcada: boolean
  importable: boolean
  avisos: string[]
  ya: number | null
}

export interface Importables {
  propuestas: Propuesta[]
  errores: string[]
}

export interface ResumenTaller {
  impresoras: number
  materiales: number
  perfiles: number
  perfiles_en_uso: number
  laminados_sin_impresora: { maquina: string | null; res_x: number; res_y: number; laminados: number }[]
}

export interface ArchivoEncontrado {
  id: number
  ruta: string
  nombre: string
  ext: string
  tipo: string
  bytes: number
}

// ---------- fase 6: impresiones, existencias y costos ----------

export type Acabado = 'sin_pintar' | 'imprimada' | 'pintada'
export type Resultado = 'bien' | 'parcial' | 'fallida'

export interface CampoAjuste {
  clave: string
  etiqueta: string
  ayuda: string
  unidad: string
  min: number
  max: number
}

export interface AjustesCostos {
  campos: CampoAjuste[]
  valores: Record<string, number>
  moneda: string
}

export interface Desglose {
  material: number | null
  luz: number | null
  limpieza: number | null
  fallas: number | null
  total: number
  consumo_material: number | null
  faltan: string[]
}

export interface PrecioSugerido {
  costo: number | null
  acabado: number
  empaque: number
  margen: number | null
  sugerido: number | null
  faltan: string[]
}

export interface Calculo {
  desglose: Desglose
  costo_unitario: number | null
  precio: PrecioSugerido
}

export interface PiezaPropuesta {
  modelo_id: number
  nombre: string
  faccion: string
  carpeta: string
  cantidad: number | null
  estado: 'sugerido' | 'confirmado'
}

export interface PropuestaImpresion {
  placa_id: number
  placa: string
  impresora_id: number | null
  perfil_id: number | null
  material_id: number | null
  laminado: { id: number; nombre: string } | null
  consumo: number | null
  unidad: 'ml' | 'g'
  /** «anterior»: lo que gastó la última impresión de la placa */
  consumo_origen: 'laminado' | 'anterior' | 'capturado'
  tiempo_s: number | null
  piezas: PiezaPropuesta[]
  ubicaciones: string[]
  /** pedidos abiertos a los que les faltan miniaturas de los modelos de la placa */
  pedidos: { id: number; cliente: string; faltan: number }[]
  /** con cuántas miniaturas se reparte (las de la placa o las de la última vez) */
  estimado: Calculo & { piezas: number }
}

export interface MiniaturaImpresa {
  modelo_id: number
  nombre: string
  faccion: string
  carpeta: string
  acabado: Acabado
  ubicacion: string
  cantidad: number
}

export interface Impresion {
  id: number
  placa_id: number | null
  placa: string
  impresora_id: number | null
  impresora: string | null
  material_id: number | null
  material: string | null
  perfil_id: number | null
  perfil: string | null
  perfil_version: number | null
  fecha: number
  resultado: Resultado
  consumo: number | null
  unidad: 'ml' | 'g'
  consumo_origen: 'laminado' | 'capturado'
  tiempo_s: number | null
  costo_material: number | null
  costo_luz: number | null
  costo_limpieza: number | null
  costo_fallas: number | null
  costo_total: number
  piezas: number
  material_descontado: number | null
  notas: string
  usuario: string
  miniaturas: MiniaturaImpresa[]
}

export interface ListaImpresiones {
  total: number
  costo: number
  piezas: number
  pagina: number
  por_pagina: number
  impresiones: Impresion[]
}

export interface Existencia {
  id: number
  modelo_id: number
  nombre: string
  faccion: string
  juego: string
  estudio: string
  carpeta: string
  acabado: Acabado
  ubicacion: string
  precio: number | null
  minimo: number | null
  notas: string
  cantidad: number
  costo_unitario: number | null
  ultimo: number | null
  precio_sugerido: PrecioSugerido
  precio_venta: number | null
  valor_costo: number | null
  valor_venta: number | null
  bajo_minimo: boolean
  portada: { id: number; clave: string } | null
}

export interface Movimiento {
  id: number
  delta: number
  motivo: 'impresion' | 'venta' | 'merma' | 'ajuste' | 'acabado' | 'traslado'
  costo_unitario: number | null
  impresion_id: number | null
  par_acabado: Acabado | null
  par_ubicacion: string | null
  notas: string
  usuario: string
  fecha: number
}

export interface ExistenciaFicha extends Existencia {
  movimientos: Movimiento[]
}

export interface ResumenExistencias {
  piezas: number
  existencias: number
  modelos: number
  valor_costo: number
  valor_venta: number
  sin_precio: number
  bajo_minimo: number
  ubicaciones: { ubicacion: string; piezas: number }[]
  impresiones: number
  impresiones_mes: number
  costo_mes: number
  fallidas_mes: number
  ajustes_faltan: string[]
  moneda: string
}

export interface ExistenciasModelo {
  existencias: Existencia[]
  impresiones: ListaImpresiones
  costo_promedio: number | null
  precios: Record<Acabado, PrecioSugerido>
}

// ---------- fase 7: ventas ----------

export type EstadoPedido = 'cotizado' | 'confirmado' | 'imprimiendo' | 'listo' | 'entregado' | 'cancelado'

export interface Cliente {
  id: number
  nombre: string
  whatsapp: string
  telefono: string
  correo: string
  redes: string
  notas: string
  pedidos: number
  comprado: number
  ultimo: number | null
}

export interface PedidoFila {
  id: number
  cliente_id: number | null
  cliente: string | null
  estado: EstadoPedido
  coleccion_id: number | null
  coleccion: string | null
  descuento: number
  envio: number
  fecha_entrega: number | null
  entregado: number | null
  notas: string
  creado: number
  actualizado: number
  piezas: number
  subtotal: number
  pagado: number
  apartadas: number
  costo: number | null
  total: number
  saldo: number
  faltan: number
  ganancia: number | null
}

export interface ListaPedidos {
  total: number
  pagina: number
  por_pagina: number
  pedidos: PedidoFila[]
}

export interface LineaPedido {
  id: number
  modelo_id: number
  nombre: string
  faccion: string
  juego: string
  estudio: string
  carpeta: string
  acabado: Acabado
  cantidad: number
  precio: number
  notas: string
  apartadas: number
  costo: number | null
  placas: number
  faltan: number
  subtotal: number
  disponibles: { id: number; acabado: Acabado; ubicacion: string; cantidad: number }[] | null
  portada: { id: number; clave: string } | null
}

export interface Pago {
  id: number
  monto: number
  fecha: number
  metodo: 'efectivo' | 'transferencia' | 'tarjeta' | 'otro'
  notas: string
  usuario: string
}

export interface PedidoFicha extends PedidoFila {
  lineas: LineaPedido[]
  pagos: Pago[]
  datos_cliente: { nombre: string; whatsapp: string; telefono: string; correo: string; redes: string } | null
}

export interface LineaColeccion {
  id: number
  modelo_id: number
  nombre: string
  faccion: string
  juego: string
  estudio: string
  carpeta: string
  acabado: Acabado
  cantidad: number
  precio_a_mano: number | null
  foto_id: number | null
  nota: string
  orden: number
  hay: number
  placas: number
  precio: number | null
  precio_sugerido: number | null
  costo: number | null
  subtotal: number | null
  faltan: number
  foto: { id: number; clave: string } | null
}

export interface Coleccion {
  id: number
  nombre: string
  descripcion: string
  estado: 'borrador' | 'activa' | 'archivada'
  precio_fijo: number | null
  creada: number
  actualizada: number
  piezas: number
  suma: number
  precio: number
  sin_precio: number
  costo: number | null
  ganancia: number | null
  faltan: number
  alcanza: boolean
  moneda: string
}

export interface ColeccionFila extends Coleccion {
  n_modelos: number
  fotos: { id: number; clave: string }[]
}

export interface ColeccionFicha extends Coleccion {
  lineas: LineaColeccion[]
}

export interface FotoModelo {
  id: number
  nombre: string
  tipo: string
  clave: string
}

export interface ResumenVentas {
  dias: number
  abiertos: number
  atrasados: number
  por_cobrar: number
  vendidos: number
  piezas: number
  ingresos: number
  envios: number
  costo: number
  ganancia: number
  sin_costo: number
  mas_vendidos: { modelo_id: number; nombre: string; faccion: string; juego: string; cantidad: number; ingreso: number; hay: number }[]
  facciones: { faccion: string; juego: string; cantidad: number; ingreso: number }[]
}
