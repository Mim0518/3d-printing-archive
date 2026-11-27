import { Anchor, Badge, Button, Card, Group, Modal, Stack, Table, Text } from '@mantine/core'
import { useQuery } from '@tanstack/react-query'
import { useState } from 'react'
import { Link } from 'react-router'

import { api } from '../api'
import { dinero, n, plural } from '../formato'
import { ACABADO } from '../opciones'
import { AgregarExistencia, ExplicarPrecio, FichaExistencia } from '../paginas/Existencias'
import type { Acabado, ExistenciasModelo } from '../tipos'
import { ListaDeImpresiones } from './Impresiones'

/** En la ficha de un modelo: cuántas hay impresas, dónde, cuánto cuestan y en cuánto venderlas. */
export default function ExistenciasDelModelo({ modeloId, nombre }: { modeloId: number; nombre: string }) {
  const [abierta, setAbierta] = useState<number | null>(null)
  const [agregar, setAgregar] = useState(false)
  const [historial, setHistorial] = useState(false)
  const q = useQuery({
    queryKey: ['existencias-modelo', modeloId],
    queryFn: () => api<ExistenciasModelo>(`/modelos/${modeloId}/existencias`),
  })
  const d = q.data
  if (!d) return null
  const total = d.existencias.reduce((s, e) => s + e.cantidad, 0)
  const ubicaciones = [...new Set(d.existencias.map((e) => e.ubicacion).filter(Boolean))]
  return (
    <Card withBorder>
      <Group justify="space-between" align="flex-start">
        <div>
          <Text fw={500}>Existencias</Text>
          <Text size="sm" c="dimmed">
            {total > 0 ? plural(total, 'impresa', 'impresas') : 'Ninguna impresa'}
            {d.costo_promedio != null ? ` · cuestan ${dinero(d.costo_promedio)} cada una en promedio` : ''}
          </Text>
        </div>
        <Button size="xs" variant="light" onClick={() => setAgregar(true)}>
          Agregar las que ya tenías
        </Button>
      </Group>
      {d.existencias.length > 0 && (
        <Table mt="xs" highlightOnHover>
          <Table.Tbody>
            {d.existencias.map((e) => (
              <Table.Tr key={e.id} style={{ cursor: 'pointer', opacity: e.cantidad ? 1 : 0.5 }} onClick={() => setAbierta(e.id)}>
                <Table.Td>
                  <Group gap={4}>
                    <Badge size="sm" color={ACABADO[e.acabado].color} variant="light">
                      {ACABADO[e.acabado].texto}
                    </Badge>
                    {e.ubicacion && (
                      <Badge size="sm" variant="outline" color="gray">
                        {e.ubicacion}
                      </Badge>
                    )}
                  </Group>
                </Table.Td>
                <Table.Td ta="right">
                  <Text fw={600} c={e.bajo_minimo ? 'orange' : undefined}>
                    {n(e.cantidad)}
                  </Text>
                </Table.Td>
                <Table.Td ta="right">
                  <Text size="sm">{e.precio_venta != null ? dinero(e.precio_venta) : '—'}</Text>
                </Table.Td>
              </Table.Tr>
            ))}
          </Table.Tbody>
        </Table>
      )}
      {d.costo_promedio != null && (
        <Stack gap={2} mt="xs">
          <Text size="sm" fw={500}>
            Precio sugerido por miniatura
          </Text>
          {(Object.keys(ACABADO) as Acabado[]).map((a) => (
            <div key={a}>
              <Text size="sm">
                {ACABADO[a].texto}: <b>{d.precios[a].sugerido != null ? dinero(d.precios[a].sugerido) : '—'}</b>
              </Text>
              <ExplicarPrecio p={d.precios[a]} />
            </div>
          ))}
        </Stack>
      )}
      {d.impresiones.total > 0 && (
        <Stack gap="xs" mt="xs">
          <Anchor size="sm" onClick={() => setHistorial(!historial)}>
            {historial ? 'Ocultar' : 'Ver'} {d.impresiones.total === 1 ? 'su impresión' : `sus ${n(d.impresiones.total)} impresiones`}
          </Anchor>
          {historial && <ListaDeImpresiones filtro={`modelo_id=${modeloId}`} porPagina={10} />}
        </Stack>
      )}
      {d.existencias.length === 0 && d.impresiones.total === 0 && (
        <Text size="xs" c="dimmed" mt={4}>
          Registra una impresión desde una de sus <Anchor component={Link} to={`/placas?modelo_id=${modeloId}`} size="xs">placas</Anchor>.
        </Text>
      )}
      <Modal opened={abierta !== null} onClose={() => setAbierta(null)} title="Existencia" size="lg">
        {abierta !== null && <FichaExistencia id={abierta} ubicaciones={ubicaciones} />}
      </Modal>
      <Modal opened={agregar} onClose={() => setAgregar(false)} title="Agregar miniaturas que ya tenías">
        {agregar && <AgregarExistencia modelo={{ id: modeloId, nombre }} ubicaciones={ubicaciones} onListo={() => setAgregar(false)} />}
      </Modal>
    </Card>
  )
}
