import { useQuery } from '@tanstack/react-query'
import { useEffect, useState } from 'react'
import { Link, useParams } from 'react-router'

import { api } from '../api'
import { titulo } from '../formato'
import { ACABADO } from '../opciones'
import type { ColeccionFicha } from '../tipos'

const dinero = (x: number) =>
  new Intl.NumberFormat('es-MX', { style: 'currency', currency: 'MXN', maximumFractionDigits: 0 }).format(x)

/** La colección en una hoja carta lista para imprimir o «Guardar como PDF» (fuera del marco de la app). */
export default function HojaColeccion() {
  const { id } = useParams()
  const q = useQuery({ queryKey: ['coleccion', id], queryFn: () => api<ColeccionFicha>(`/colecciones/${id}`) })
  const [hoy] = useState(() => new Date())
  useEffect(() => {
    if (q.data) document.title = q.data.nombre
  }, [q.data])
  if (q.isPending) return <p style={{ padding: 24 }}>Cargando…</p>
  if (q.error) return <p style={{ padding: 24 }}>{q.error.message}</p>
  const c = q.data
  return (
    <div className="hoja-fondo">
      <style>{`
        .hoja-fondo { background: #e9ecef; min-height: 100vh; padding: 16px; color: #1a1b1e; }
        .hoja { background: #fff; max-width: 8.5in; margin: 0 auto; padding: 0.5in; box-shadow: 0 2px 12px rgba(0,0,0,.15);
                font-family: system-ui, -apple-system, "Segoe UI", sans-serif; }
        .hoja h1 { font-size: 26pt; margin: 0 0 6pt; }
        .hoja .desc { font-size: 11pt; color: #495057; white-space: pre-wrap; margin: 0 0 14pt; }
        .rejilla { display: grid; grid-template-columns: repeat(3, 1fr); gap: 12pt; }
        .tarjeta { break-inside: avoid; }
        .tarjeta .foto { aspect-ratio: 1; background: #f1f3f5; display: flex; align-items: center; justify-content: center;
                         border-radius: 6px; overflow: hidden; }
        .tarjeta img { width: 100%; height: 100%; object-fit: contain; }
        .tarjeta .nombre { font-weight: 700; font-size: 11pt; margin-top: 4pt; }
        .tarjeta .det { font-size: 9pt; color: #495057; }
        .tarjeta .precio { font-size: 10pt; font-weight: 600; }
        .total { display: flex; justify-content: space-between; align-items: baseline; border-top: 2px solid #dee2e6;
                 margin-top: 16pt; padding-top: 10pt; }
        .total .cifra { font-size: 22pt; font-weight: 800; }
        .total .nota { font-size: 9pt; color: #495057; }
        .barra { max-width: 8.5in; margin: 0 auto 12px; display: flex; gap: 8px; align-items: center;
                 font-family: system-ui, sans-serif; }
        .barra button, .barra a { font: inherit; padding: 6px 14px; border-radius: 6px; border: 1px solid #adb5bd;
                 background: #fff; color: #1a1b1e; text-decoration: none; cursor: pointer; }
        .barra button { background: #228be6; color: #fff; border-color: #228be6; }
        @media print {
          @page { size: letter; margin: 0.5in; }
          .hoja-fondo { background: #fff; padding: 0; }
          .hoja { box-shadow: none; padding: 0; max-width: none; }
          .barra { display: none; }
        }
      `}</style>
      <div className="barra">
        <button type="button" onClick={() => window.print()}>
          Imprimir o guardar como PDF
        </button>
        <Link to={`/ventas/colecciones/${c.id}`}>Volver a la colección</Link>
        <span style={{ fontSize: 13, color: '#495057' }}>En el diálogo de impresión elige «Guardar como PDF».</span>
      </div>
      <div className="hoja">
        <h1>{c.nombre}</h1>
        {c.descripcion && <p className="desc">{c.descripcion}</p>}
        <div className="rejilla">
          {c.lineas.map((l) => (
            <div key={l.id} className="tarjeta">
              <div className="foto">
                {l.foto && <img src={`/api/v1/archivos/${l.foto.id}/miniatura?t=512&v=${l.foto.clave}`} alt={titulo(l)} />}
              </div>
              <div className="nombre">
                {l.cantidad > 1 ? `${l.cantidad} × ` : ''}
                {titulo(l)}
              </div>
              <div className="det">{[l.faccion, ACABADO[l.acabado].texto].filter(Boolean).join(' · ')}</div>
              {l.nota && <div className="det">{l.nota}</div>}
              {c.precio_fijo == null && l.precio != null && <div className="precio">{dinero(l.precio * l.cantidad)}</div>}
            </div>
          ))}
        </div>
        <div className="total">
          <span className="cifra">{dinero(c.precio)}</span>
          <span className="nota">
            {c.piezas} {c.piezas === 1 ? 'miniatura' : 'miniaturas'} · precio en {c.moneda} ·{' '}
            {new Intl.DateTimeFormat('es-MX', { dateStyle: 'long' }).format(hoy)}
          </span>
        </div>
      </div>
    </div>
  )
}
