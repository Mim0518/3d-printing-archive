// La imagen de una colección para mandarla por WhatsApp o redes: se dibuja en un canvas con las miniaturas de
// la app (mismo origen), sin servicios externos.

import { titulo } from './formato'
import { ACABADO } from './opciones'
import type { ColeccionFicha } from './tipos'

const ANCHO = 1080
const MARGEN = 48
const ESPACIO = 24
const COLUMNAS = 2
const LADO = (ANCHO - MARGEN * 2 - ESPACIO * (COLUMNAS - 1)) / COLUMNAS

const dinero = (x: number) =>
  new Intl.NumberFormat('es-MX', { style: 'currency', currency: 'MXN', maximumFractionDigits: 0 }).format(x)

function cargar(src: string): Promise<HTMLImageElement | null> {
  return new Promise((ok) => {
    const img = new Image()
    img.onload = () => ok(img)
    img.onerror = () => ok(null)
    img.src = src
  })
}

/** Corta el texto en renglones que caben en `ancho` (como mucho `max`; el último lleva «…» si no cupo). */
function renglones(ctx: CanvasRenderingContext2D, texto: string, ancho: number, max: number): string[] {
  const salida: string[] = []
  for (const parrafo of texto.split('\n')) {
    let linea = ''
    for (const palabra of parrafo.split(/\s+/).filter(Boolean)) {
      const prueba = linea ? `${linea} ${palabra}` : palabra
      if (ctx.measureText(prueba).width > ancho && linea) {
        salida.push(linea)
        linea = palabra
      } else {
        linea = prueba
      }
    }
    if (linea) salida.push(linea)
  }
  if (salida.length > max) {
    const corto = salida.slice(0, max)
    corto[max - 1] = `${corto[max - 1].replace(/\s+\S*$/, '')}…`
    return corto
  }
  return salida
}

export async function imagenColeccion(c: ColeccionFicha): Promise<Blob> {
  const lienzo = document.createElement('canvas')
  const ctx = lienzo.getContext('2d')!
  const fuente = (tam: number, negritas = false) => `${negritas ? '700 ' : ''}${tam}px system-ui, -apple-system, "Segoe UI", sans-serif`

  ctx.font = fuente(30)
  const descripcion = c.descripcion ? renglones(ctx, c.descripcion, ANCHO - MARGEN * 2, 5) : []
  ctx.font = fuente(56, true)
  const tituloLineas = renglones(ctx, c.nombre, ANCHO - MARGEN * 2, 2)
  const alto_encabezado = MARGEN + tituloLineas.length * 66 + (descripcion.length ? 16 + descripcion.length * 40 : 0) + 32
  const alto_pie = 150
  const alto_texto = 140
  const filas = Math.ceil(c.lineas.length / COLUMNAS)
  lienzo.width = ANCHO
  lienzo.height = alto_encabezado + filas * (LADO + alto_texto + ESPACIO) + alto_pie

  // fondo y encabezado
  ctx.fillStyle = '#ffffff'
  ctx.fillRect(0, 0, lienzo.width, lienzo.height)
  ctx.fillStyle = '#1a1b1e'
  ctx.textBaseline = 'top'
  let y = MARGEN
  ctx.font = fuente(56, true)
  for (const l of tituloLineas) {
    ctx.fillText(l, MARGEN, y)
    y += 66
  }
  if (descripcion.length) {
    y += 16
    ctx.font = fuente(30)
    ctx.fillStyle = '#495057'
    for (const l of descripcion) {
      ctx.fillText(l, MARGEN, y)
      y += 40
    }
  }

  // las miniaturas
  const imagenes = await Promise.all(
    c.lineas.map((l) => (l.foto ? cargar(`/api/v1/archivos/${l.foto.id}/miniatura?t=512&v=${l.foto.clave}`) : Promise.resolve(null))),
  )
  c.lineas.forEach((l, i) => {
    const x = MARGEN + (i % COLUMNAS) * (LADO + ESPACIO)
    const yy = alto_encabezado + Math.floor(i / COLUMNAS) * (LADO + alto_texto + ESPACIO)
    ctx.fillStyle = '#f1f3f5'
    ctx.fillRect(x, yy, LADO, LADO)
    const img = imagenes[i]
    if (img) {
      const escala = Math.min(LADO / img.width, LADO / img.height)
      const w = img.width * escala
      const h = img.height * escala
      ctx.drawImage(img, x + (LADO - w) / 2, yy + (LADO - h) / 2, w, h)
    }
    ctx.fillStyle = '#1a1b1e'
    ctx.font = fuente(30, true)
    let ty = yy + LADO + 14
    for (const r of renglones(ctx, `${l.cantidad > 1 ? `${l.cantidad} × ` : ''}${titulo(l)}`, LADO, 2)) {
      ctx.fillText(r, x, ty)
      ty += 38
    }
    ctx.font = fuente(24)
    ctx.fillStyle = '#495057'
    ctx.fillText([l.faccion, ACABADO[l.acabado].texto].filter(Boolean).join(' · '), x, ty + 2)
    if (c.precio_fijo == null && l.precio != null) {
      ctx.font = fuente(28, true)
      ctx.fillStyle = '#1a1b1e'
      ctx.textAlign = 'right'
      ctx.fillText(dinero(l.precio * l.cantidad), x + LADO, ty + 34)
      ctx.textAlign = 'left'
    }
  })

  // el precio
  const yp = lienzo.height - alto_pie + 30
  ctx.fillStyle = '#e9ecef'
  ctx.fillRect(MARGEN, yp - 20, ANCHO - MARGEN * 2, 2)
  ctx.fillStyle = '#1a1b1e'
  ctx.font = fuente(52, true)
  ctx.fillText(`${dinero(c.precio)}`, MARGEN, yp)
  ctx.font = fuente(26)
  ctx.fillStyle = '#495057'
  ctx.textAlign = 'right'
  ctx.fillText(`${c.piezas} ${c.piezas === 1 ? 'miniatura' : 'miniaturas'} · precio en ${c.moneda}`, ANCHO - MARGEN, yp + 18)
  ctx.textAlign = 'left'

  return new Promise((ok, mal) => lienzo.toBlob((b) => (b ? ok(b) : mal(new Error('No se pudo crear la imagen'))), 'image/png'))
}

/** Comparte la imagen (en el celular abre WhatsApp y las demás apps) o, si no se puede, la descarga. */
export async function compartirODescargar(blob: Blob, nombre: string): Promise<'compartida' | 'descargada'> {
  const archivo = new File([blob], `${nombre}.png`, { type: 'image/png' })
  if (navigator.canShare?.({ files: [archivo] })) {
    try {
      await navigator.share({ files: [archivo], title: nombre })
      return 'compartida'
    } catch {
      // cancelado: se descarga
    }
  }
  const url = URL.createObjectURL(blob)
  const a = document.createElement('a')
  a.href = url
  a.download = `${nombre}.png`
  a.click()
  setTimeout(() => URL.revokeObjectURL(url), 10_000)
  return 'descargada'
}
