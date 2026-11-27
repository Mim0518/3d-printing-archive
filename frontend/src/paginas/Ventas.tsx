import {
  Alert,
  Anchor,
  Badge,
  Button,
  Card,
  Group,
  Loader,
  Modal,
  Pagination,
  SegmentedControl,
  Select,
  SimpleGrid,
  Stack,
  Table,
  Tabs,
  Text,
  TextInput,
  Textarea,
  Title,
} from '@mantine/core'
import { useDebouncedValue } from '@mantine/hooks'
import { keepPreviousData, useMutation, useQuery } from '@tanstack/react-query'
import { useState } from 'react'
import { Link, useNavigate, useSearchParams } from 'react-router'

import { api } from '../api'
import Miniatura from '../componentes/Miniatura'
import { cuando, dinero, n, plural } from '../formato'
import { ESTADO_COLECCION, ESTADO_PEDIDO, useRefrescarVentas } from '../opciones'
import type { Cliente, ColeccionFila, ListaPedidos, ResumenVentas } from '../tipos'

function fecha(segundos: number | null): string {
  if (!segundos) return ''
  return new Intl.DateTimeFormat('es-MX', { dateStyle: 'medium' }).format(new Date(segundos * 1000))
}

// ---------- pedidos ----------

function NuevoPedido({ onListo }: { onListo: (id: number) => void }) {
  const refrescar = useRefrescarVentas()
  const clientes = useQuery({ queryKey: ['clientes', ''], queryFn: () => api<Cliente[]>('/clientes') })
  const [cliente, setCliente] = useState<string | null>(null)
  const [nuevo, setNuevo] = useState('')
  const [notas, setNotas] = useState('')
  const crear = useMutation({
    mutationFn: () =>
      api<{ id: number }>('/pedidos', {
        method: 'POST',
        body: JSON.stringify({
          cliente_id: cliente ? Number(cliente) : null,
          cliente: !cliente && nuevo.trim() ? { nombre: nuevo } : null,
          notas,
        }),
      }),
    onSuccess: (r) => {
      refrescar()
      onListo(r.id)
    },
  })
  return (
    <Stack gap="sm">
      <Select
        label="Cliente"
        placeholder="Elige uno, o escribe abajo uno nuevo"
        searchable
        clearable
        data={(clientes.data ?? []).map((c) => ({ value: String(c.id), label: c.nombre }))}
        value={cliente}
        onChange={setCliente}
      />
      {!cliente && <TextInput label="…o un cliente nuevo" placeholder="Nombre" value={nuevo} onChange={(e) => setNuevo(e.currentTarget.value)} />}
      <Textarea label="Notas" autosize minRows={2} value={notas} onChange={(e) => setNotas(e.currentTarget.value)} />
      {crear.error && <Alert color="red">{crear.error.message}</Alert>}
      <Button loading={crear.isPending} onClick={() => crear.mutate()}>
        Crear pedido y agregar sus piezas
      </Button>
    </Stack>
  )
}

function PedidosTab() {
  const [params, setParams] = useSearchParams()
  const navegar = useNavigate()
  const [texto, setTexto] = useState('')
  const [buscado] = useDebouncedValue(texto, 300)
  const [pagina, setPagina] = useState(1)
  const [nuevo, setNuevo] = useState(false)
  const [ahora] = useState(() => Date.now())
  const estado = params.get('estado') ?? 'abiertos'
  const consulta = new URLSearchParams({ pagina: String(pagina), por_pagina: '30' })
  if (estado !== 'todos') consulta.set('estado', estado)
  if (buscado) consulta.set('q', buscado)
  const q = useQuery({
    queryKey: ['pedidos', consulta.toString()],
    queryFn: () => api<ListaPedidos>(`/pedidos?${consulta}`),
    placeholderData: keepPreviousData,
  })
  return (
    <Stack>
      <Group gap="xs" wrap="wrap">
        <TextInput placeholder="Cliente, modelo o nota…" value={texto} onChange={(e) => setTexto(e.currentTarget.value)} style={{ flex: 1 }} miw={200} />
        <Select
          data={[
            { value: 'abiertos', label: 'Abiertos' },
            { value: 'todos', label: 'Todos' },
            ...Object.entries(ESTADO_PEDIDO).map(([value, e]) => ({ value, label: e.texto })),
          ]}
          value={estado}
          allowDeselect={false}
          onChange={(v) => {
            setPagina(1)
            setParams(v && v !== 'abiertos' ? { ver: 'pedidos', estado: v } : {}, { replace: true })
          }}
          w={160}
        />
        <Button onClick={() => setNuevo(true)}>Nuevo pedido</Button>
      </Group>
      {q.isPending ? (
        <Loader />
      ) : q.error ? (
        <Alert color="red">{q.error.message}</Alert>
      ) : q.data.pedidos.length === 0 ? (
        <Text c="dimmed" ta="center" py="lg">
          No hay pedidos {estado === 'abiertos' ? 'abiertos' : 'que coincidan'}.
        </Text>
      ) : (
        <Table.ScrollContainer minWidth={720}>
          <Table highlightOnHover style={{ opacity: q.isPlaceholderData ? 0.6 : 1 }}>
            <Table.Thead>
              <Table.Tr>
                <Table.Th>Pedido</Table.Th>
                <Table.Th>Estado</Table.Th>
                <Table.Th ta="right">Miniaturas</Table.Th>
                <Table.Th ta="right">Total</Table.Th>
                <Table.Th ta="right">Por cobrar</Table.Th>
                <Table.Th>Entrega</Table.Th>
              </Table.Tr>
            </Table.Thead>
            <Table.Tbody>
              {q.data.pedidos.map((p) => (
                <Table.Tr key={p.id} style={{ cursor: 'pointer' }} onClick={() => navegar(`/ventas/pedidos/${p.id}`)}>
                  <Table.Td>
                    <Text size="sm" fw={500}>
                      #{p.id} {p.cliente ?? 'sin cliente'}
                    </Text>
                    <Text size="xs" c="dimmed" lineClamp={1}>
                      {p.coleccion ?? p.notas}
                    </Text>
                  </Table.Td>
                  <Table.Td>
                    <Badge color={ESTADO_PEDIDO[p.estado].color} variant="light">
                      {ESTADO_PEDIDO[p.estado].texto}
                    </Badge>
                  </Table.Td>
                  <Table.Td ta="right">
                    <Text size="sm">{n(p.piezas)}</Text>
                    {p.faltan > 0 && p.estado !== 'cancelado' && (
                      <Text size="xs" c="orange">
                        faltan {n(p.faltan)}
                      </Text>
                    )}
                  </Table.Td>
                  <Table.Td ta="right">{dinero(p.total)}</Table.Td>
                  <Table.Td ta="right">
                    <Text size="sm" c={p.saldo > 0 ? 'orange' : 'dimmed'}>
                      {p.saldo > 0 ? dinero(p.saldo) : 'pagado'}
                    </Text>
                  </Table.Td>
                  <Table.Td>
                    <Text size="sm" c={p.fecha_entrega && !p.entregado && p.fecha_entrega * 1000 < ahora ? 'red' : undefined}>
                      {p.entregado ? `entregado ${fecha(p.entregado)}` : fecha(p.fecha_entrega)}
                    </Text>
                  </Table.Td>
                </Table.Tr>
              ))}
            </Table.Tbody>
          </Table>
        </Table.ScrollContainer>
      )}
      {q.data && q.data.total > 30 && <Pagination total={Math.ceil(q.data.total / 30)} value={pagina} onChange={setPagina} />}
      <Modal opened={nuevo} onClose={() => setNuevo(false)} title="Nuevo pedido">
        {nuevo && <NuevoPedido onListo={(id) => navegar(`/ventas/pedidos/${id}`)} />}
      </Modal>
    </Stack>
  )
}

// ---------- clientes ----------

export function FormCliente({ c, onListo }: { c?: Partial<Cliente>; onListo: () => void }) {
  const refrescar = useRefrescarVentas()
  const navegar = useNavigate()
  const [f, setF] = useState({
    nombre: c?.nombre ?? '',
    whatsapp: c?.whatsapp ?? '',
    telefono: c?.telefono ?? '',
    correo: c?.correo ?? '',
    redes: c?.redes ?? '',
    notas: c?.notas ?? '',
  })
  const poner = (x: Partial<typeof f>) => setF({ ...f, ...x })
  const guardar = useMutation({
    mutationFn: () =>
      c?.id
        ? api(`/clientes/${c.id}`, { method: 'PATCH', body: JSON.stringify(f) })
        : api('/clientes', { method: 'POST', body: JSON.stringify(f) }),
    onSuccess: () => {
      refrescar()
      onListo()
    },
  })
  const borrar = useMutation({
    mutationFn: () => api(`/clientes/${c?.id}`, { method: 'DELETE' }),
    onSuccess: () => {
      refrescar()
      onListo()
    },
  })
  const pedidos = useQuery({
    queryKey: ['pedidos', 'cliente', c?.id],
    queryFn: () => api<ListaPedidos>(`/pedidos?cliente_id=${c?.id}&estado=`),
    enabled: !!c?.id,
  })
  return (
    <Stack gap="sm">
      <TextInput label="Nombre" required value={f.nombre} onChange={(e) => poner({ nombre: e.currentTarget.value })} />
      <SimpleGrid cols={{ base: 1, xs: 2 }}>
        <TextInput label="WhatsApp" value={f.whatsapp} onChange={(e) => poner({ whatsapp: e.currentTarget.value })} />
        <TextInput label="Teléfono" value={f.telefono} onChange={(e) => poner({ telefono: e.currentTarget.value })} />
        <TextInput label="Correo" value={f.correo} onChange={(e) => poner({ correo: e.currentTarget.value })} />
        <TextInput label="Redes" placeholder="@usuario, Facebook, Mercado Libre…" value={f.redes} onChange={(e) => poner({ redes: e.currentTarget.value })} />
      </SimpleGrid>
      <Textarea label="Notas" autosize minRows={2} value={f.notas} onChange={(e) => poner({ notas: e.currentTarget.value })} />
      {(guardar.error ?? borrar.error) && <Alert color="red">{(guardar.error ?? borrar.error)?.message}</Alert>}
      <Group justify="space-between">
        <Button loading={guardar.isPending} disabled={!f.nombre.trim()} onClick={() => guardar.mutate()}>
          {c?.id ? 'Guardar' : 'Agregar cliente'}
        </Button>
        {c?.id && !c.pedidos && (
          <Button variant="subtle" color="red" loading={borrar.isPending} onClick={() => borrar.mutate()}>
            Borrar
          </Button>
        )}
      </Group>
      {pedidos.data && pedidos.data.pedidos.length > 0 && (
        <Stack gap={4}>
          <Text size="sm" fw={500}>
            Sus pedidos
          </Text>
          {pedidos.data.pedidos.map((p) => (
            <Group key={p.id} gap="xs" style={{ cursor: 'pointer' }} onClick={() => navegar(`/ventas/pedidos/${p.id}`)}>
              <Anchor size="sm">#{p.id}</Anchor>
              <Badge size="xs" color={ESTADO_PEDIDO[p.estado].color} variant="light">
                {ESTADO_PEDIDO[p.estado].texto}
              </Badge>
              <Text size="sm">{dinero(p.total)}</Text>
              <Text size="xs" c="dimmed">
                {cuando(p.creado)}
              </Text>
            </Group>
          ))}
        </Stack>
      )}
    </Stack>
  )
}

function ClientesTab() {
  const [texto, setTexto] = useState('')
  const [buscado] = useDebouncedValue(texto, 300)
  const [abierto, setAbierto] = useState<Partial<Cliente> | null>(null)
  const q = useQuery({
    queryKey: ['clientes', buscado],
    queryFn: () => api<Cliente[]>(`/clientes?q=${encodeURIComponent(buscado)}`),
    placeholderData: keepPreviousData,
  })
  return (
    <Stack>
      <Group gap="xs">
        <TextInput placeholder="Nombre, WhatsApp, redes…" value={texto} onChange={(e) => setTexto(e.currentTarget.value)} style={{ flex: 1 }} />
        <Button onClick={() => setAbierto({})}>Nuevo cliente</Button>
      </Group>
      {q.data && q.data.length === 0 && (
        <Text c="dimmed" ta="center" py="lg">
          Todavía no hay clientes.
        </Text>
      )}
      {q.data && q.data.length > 0 && (
        <Table.ScrollContainer minWidth={560}>
          <Table highlightOnHover>
            <Table.Tbody>
              {q.data.map((c) => (
                <Table.Tr key={c.id} style={{ cursor: 'pointer' }} onClick={() => setAbierto(c)}>
                  <Table.Td>
                    <Text size="sm" fw={500}>
                      {c.nombre}
                    </Text>
                    <Text size="xs" c="dimmed">
                      {[c.whatsapp && `WhatsApp ${c.whatsapp}`, c.telefono, c.correo, c.redes].filter(Boolean).join(' · ')}
                    </Text>
                  </Table.Td>
                  <Table.Td ta="right">
                    <Text size="sm">{plural(c.pedidos, 'pedido', 'pedidos')}</Text>
                    {c.comprado > 0 && (
                      <Text size="xs" c="dimmed">
                        {dinero(c.comprado)} comprado
                      </Text>
                    )}
                  </Table.Td>
                </Table.Tr>
              ))}
            </Table.Tbody>
          </Table>
        </Table.ScrollContainer>
      )}
      <Modal opened={abierto !== null} onClose={() => setAbierto(null)} title={abierto?.id ? abierto.nombre : 'Nuevo cliente'} size="lg">
        {abierto && <FormCliente key={abierto.id ?? 'nuevo'} c={abierto} onListo={() => setAbierto(null)} />}
      </Modal>
    </Stack>
  )
}

// ---------- colecciones ----------

function ColeccionesTab() {
  const navegar = useNavigate()
  const refrescar = useRefrescarVentas()
  const [estado, setEstado] = useState<string>('')
  const q = useQuery({
    queryKey: ['colecciones', estado],
    queryFn: () => api<ColeccionFila[]>(`/colecciones?estado=${estado}`),
  })
  const crear = useMutation({
    mutationFn: () => api<{ id: number }>('/colecciones', { method: 'POST', body: JSON.stringify({ nombre: 'Colección nueva' }) }),
    onSuccess: (r) => {
      refrescar()
      navegar(`/ventas/colecciones/${r.id}`)
    },
  })
  return (
    <Stack>
      <Group justify="space-between">
        <SegmentedControl
          size="xs"
          value={estado}
          onChange={setEstado}
          data={[
            { value: '', label: 'Todas' },
            { value: 'activa', label: 'Activas' },
            { value: 'borrador', label: 'Borradores' },
            { value: 'archivada', label: 'Archivadas' },
          ]}
        />
        <Button loading={crear.isPending} onClick={() => crear.mutate()}>
          Nueva colección
        </Button>
      </Group>
      {q.data && q.data.length === 0 && (
        <Text c="dimmed" ta="center" py="lg">
          No hay colecciones todavía. Una colección junta modelos para ofrecerlos con fotos y precio, y se manda en PDF o
          imagen.
        </Text>
      )}
      <SimpleGrid cols={{ base: 1, xs: 2, md: 3, lg: 4 }}>
        {(q.data ?? []).map((c) => (
          <Card key={c.id} withBorder padding="xs" component={Link} to={`/ventas/colecciones/${c.id}`} style={{ textDecoration: 'none' }}>
            <Card.Section>
              <SimpleGrid cols={2} spacing={1}>
                {[0, 1, 2, 3].map((i) => (
                  <Miniatura key={i} archivoId={c.fotos[i]?.id ?? 0} clave={c.fotos[i]?.clave} tamano={128} lado={120} lleno />
                ))}
              </SimpleGrid>
            </Card.Section>
            <Group justify="space-between" mt="xs" wrap="nowrap">
              <Text fw={600} lineClamp={1}>
                {c.nombre}
              </Text>
              <Badge size="xs" color={ESTADO_COLECCION[c.estado].color} variant="light">
                {ESTADO_COLECCION[c.estado].texto}
              </Badge>
            </Group>
            <Text size="sm">
              {dinero(c.precio)} · {plural(c.piezas, 'miniatura', 'miniaturas')}
            </Text>
            <Text size="xs" c={c.alcanza ? 'green' : 'orange'}>
              {c.n_modelos === 0 ? 'sin modelos' : c.alcanza ? 'alcanzan las existencias' : `faltan ${n(c.faltan)} por imprimir`}
            </Text>
          </Card>
        ))}
      </SimpleGrid>
    </Stack>
  )
}

// ---------- resumen ----------

function Dato({ titulo, valor, detalle, color }: { titulo: string; valor: string; detalle?: string; color?: string }) {
  return (
    <Card withBorder padding="sm">
      <Text size="xs" c="dimmed" tt="uppercase" fw={600}>
        {titulo}
      </Text>
      <Text size="xl" fw={700} c={color}>
        {valor}
      </Text>
      {detalle && (
        <Text size="xs" c="dimmed">
          {detalle}
        </Text>
      )}
    </Card>
  )
}

function ResumenTab() {
  const [dias, setDias] = useState('90')
  const q = useQuery({ queryKey: ['ventas-resumen', dias], queryFn: () => api<ResumenVentas>(`/ventas/resumen?dias=${dias}`) })
  if (q.isPending) return <Loader />
  if (q.error) return <Alert color="red">{q.error.message}</Alert>
  const r = q.data
  return (
    <Stack>
      <SegmentedControl
        size="xs"
        value={dias}
        onChange={setDias}
        data={[
          { value: '30', label: '30 días' },
          { value: '90', label: '90 días' },
          { value: '365', label: 'Un año' },
          { value: '3650', label: 'Todo' },
        ]}
        w="fit-content"
      />
      <SimpleGrid cols={{ base: 2, md: 4 }}>
        <Dato titulo="Vendido" valor={dinero(r.ingresos)} detalle={`${plural(r.vendidos, 'pedido entregado', 'pedidos entregados')} · ${plural(r.piezas, 'miniatura', 'miniaturas')}`} />
        <Dato
          titulo="Ganancia"
          valor={dinero(r.ganancia)}
          detalle={r.sin_costo ? `${plural(r.sin_costo, 'pedido', 'pedidos')} sin costo completo` : `costo ${dinero(r.costo)}`}
          color="green"
        />
        <Dato titulo="Por cobrar" valor={dinero(r.por_cobrar)} color={r.por_cobrar > 0 ? 'orange' : undefined} />
        <Dato
          titulo="Abiertos"
          valor={n(r.abiertos)}
          detalle={r.atrasados ? `${plural(r.atrasados, 'atrasado', 'atrasados')}` : 'ninguno atrasado'}
          color={r.atrasados ? 'red' : undefined}
        />
      </SimpleGrid>
      <SimpleGrid cols={{ base: 1, md: 2 }}>
        <Card withBorder>
          <Text fw={500} mb="xs">
            Lo más vendido
          </Text>
          {r.mas_vendidos.length === 0 ? (
            <Text size="sm" c="dimmed">
              Nada vendido en este periodo.
            </Text>
          ) : (
            <Table>
              <Table.Tbody>
                {r.mas_vendidos.map((m) => (
                  <Table.Tr key={m.modelo_id}>
                    <Table.Td>
                      <Anchor component={Link} to={`/catalogo/${m.modelo_id}`} size="sm">
                        {m.nombre || m.faccion}
                      </Anchor>
                      <Text size="xs" c="dimmed">
                        {m.faccion}
                      </Text>
                    </Table.Td>
                    <Table.Td ta="right">{n(m.cantidad)}</Table.Td>
                    <Table.Td ta="right">{dinero(m.ingreso)}</Table.Td>
                    <Table.Td ta="right">
                      {m.hay === 0 ? (
                        <Badge size="xs" color="orange" variant="light">
                          no hay impresas
                        </Badge>
                      ) : (
                        <Text size="xs" c="dimmed">
                          hay {n(m.hay)}
                        </Text>
                      )}
                    </Table.Td>
                  </Table.Tr>
                ))}
              </Table.Tbody>
            </Table>
          )}
        </Card>
        <Card withBorder>
          <Text fw={500} mb="xs">
            Por facción
          </Text>
          <Table>
            <Table.Tbody>
              {r.facciones.map((f) => (
                <Table.Tr key={`${f.juego}-${f.faccion}`}>
                  <Table.Td>
                    <Text size="sm">{f.faccion || '(sin facción)'}</Text>
                    <Text size="xs" c="dimmed">
                      {f.juego}
                    </Text>
                  </Table.Td>
                  <Table.Td ta="right">{n(f.cantidad)}</Table.Td>
                  <Table.Td ta="right">{dinero(f.ingreso)}</Table.Td>
                </Table.Tr>
              ))}
            </Table.Tbody>
          </Table>
        </Card>
      </SimpleGrid>
    </Stack>
  )
}

export default function Ventas() {
  const [params, setParams] = useSearchParams()
  const ver = ['clientes', 'colecciones', 'resumen'].includes(params.get('ver') ?? '') ? params.get('ver')! : 'pedidos'
  return (
    <Stack maw={1200}>
      <Title order={2}>Pedidos y colecciones</Title>
      <Tabs value={ver} onChange={(v) => setParams(v && v !== 'pedidos' ? { ver: v } : {}, { replace: true })}>
        <Tabs.List>
          <Tabs.Tab value="pedidos">Pedidos</Tabs.Tab>
          <Tabs.Tab value="clientes">Clientes</Tabs.Tab>
          <Tabs.Tab value="colecciones">Colecciones</Tabs.Tab>
          <Tabs.Tab value="resumen">Resumen</Tabs.Tab>
        </Tabs.List>
        <Tabs.Panel value="pedidos" pt="md">
          {ver === 'pedidos' && <PedidosTab />}
        </Tabs.Panel>
        <Tabs.Panel value="clientes" pt="md">
          {ver === 'clientes' && <ClientesTab />}
        </Tabs.Panel>
        <Tabs.Panel value="colecciones" pt="md">
          {ver === 'colecciones' && <ColeccionesTab />}
        </Tabs.Panel>
        <Tabs.Panel value="resumen" pt="md">
          {ver === 'resumen' && <ResumenTab />}
        </Tabs.Panel>
      </Tabs>
    </Stack>
  )
}
