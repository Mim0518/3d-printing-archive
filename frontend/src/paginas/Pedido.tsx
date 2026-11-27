import {
  ActionIcon,
  Alert,
  Anchor,
  Badge,
  Breadcrumbs,
  Button,
  Card,
  Group,
  Loader,
  Menu,
  NumberInput,
  Select,
  SimpleGrid,
  Stack,
  Table,
  Text,
  TextInput,
  Textarea,
  Title,
} from '@mantine/core'
import { useMutation, useQuery } from '@tanstack/react-query'
import { useState } from 'react'
import { Link, useNavigate, useParams } from 'react-router'

import { api } from '../api'
import BuscarModelo from '../componentes/BuscarModelo'
import { ListaDeImpresiones } from '../componentes/Impresiones'
import Miniatura from '../componentes/Miniatura'
import { cuando, dinero, n, titulo } from '../formato'
import { ACABADO, ESTADO_PEDIDO, METODO_PAGO, aNumero, useRefrescarVentas } from '../opciones'
import type { Acabado, Cliente, EstadoPedido, LineaPedido, PedidoFicha } from '../tipos'

const PASOS: EstadoPedido[] = ['cotizado', 'confirmado', 'imprimiendo', 'listo', 'entregado']

/** «yyyy-mm-dd» ↔ segundos (para la fecha de entrega). */
function aFecha(s: number | null): string {
  if (!s) return ''
  const d = new Date(s * 1000)
  d.setMinutes(d.getMinutes() - d.getTimezoneOffset())
  return d.toISOString().slice(0, 10)
}

function deFecha(t: string): number | null {
  return t ? Math.floor(new Date(`${t}T12:00:00`).getTime() / 1000) : null
}

function Estado({ p }: { p: PedidoFicha }) {
  const refrescar = useRefrescarVentas()
  const [aviso, setAviso] = useState<string | null>(null)
  const cambiar = useMutation({
    mutationFn: (x: { estado: EstadoPedido; forzar?: boolean }) =>
      api(`/pedidos/${p.id}`, { method: 'PATCH', body: JSON.stringify(x) }),
    onSuccess: () => {
      setAviso(null)
      refrescar()
    },
    onError: (e, x) => {
      if (x.estado === 'entregado' && !x.forzar) setAviso(e.message)
    },
  })
  return (
    <Stack gap="xs">
      <Group gap={4} wrap="wrap">
        {PASOS.map((e, i) => {
          const actual = p.estado === e
          const hecho = PASOS.indexOf(p.estado as EstadoPedido) > i
          return (
            <Button
              key={e}
              size="compact-sm"
              variant={actual ? 'filled' : hecho ? 'light' : 'default'}
              color={ESTADO_PEDIDO[e].color}
              disabled={p.estado === 'cancelado' || p.estado === 'entregado'}
              loading={cambiar.isPending && cambiar.variables?.estado === e}
              onClick={() => !actual && cambiar.mutate({ estado: e })}
            >
              {ESTADO_PEDIDO[e].texto}
            </Button>
          )
        })}
        {p.estado !== 'cancelado' && p.estado !== 'entregado' && (
          <Button size="compact-sm" variant="subtle" color="red" onClick={() => cambiar.mutate({ estado: 'cancelado' })}>
            Cancelar pedido
          </Button>
        )}
        {(p.estado === 'cancelado' || p.estado === 'entregado') && (
          <Button size="compact-sm" variant="subtle" onClick={() => cambiar.mutate({ estado: 'confirmado' })}>
            Reabrir
          </Button>
        )}
      </Group>
      {p.estado === 'cancelado' && (
        <Text size="xs" c="dimmed">
          Cancelado: lo que estaba apartado regresó a las existencias.
        </Text>
      )}
      {aviso && (
        <Alert color="orange">
          <Text size="sm">{aviso}</Text>
          <Button size="xs" mt="xs" color="orange" onClick={() => cambiar.mutate({ estado: 'entregado', forzar: true })}>
            Entregar de todos modos
          </Button>
        </Alert>
      )}
      {cambiar.error && !aviso && <Alert color="red">{cambiar.error.message}</Alert>}
    </Stack>
  )
}

function Datos({ p }: { p: PedidoFicha }) {
  const refrescar = useRefrescarVentas()
  const clientes = useQuery({ queryKey: ['clientes', ''], queryFn: () => api<Cliente[]>('/clientes') })
  const inicial = () => ({
    cliente: p.cliente_id ? String(p.cliente_id) : null,
    entrega: aFecha(p.fecha_entrega),
    descuento: p.descuento as number | string,
    envio: p.envio as number | string,
    notas: p.notas,
  })
  const [f, setF] = useState(inicial)
  const cambio =
    f.cliente !== (p.cliente_id ? String(p.cliente_id) : null) ||
    f.entrega !== aFecha(p.fecha_entrega) ||
    (aNumero(f.descuento) ?? 0) !== p.descuento ||
    (aNumero(f.envio) ?? 0) !== p.envio ||
    f.notas !== p.notas
  const guardar = useMutation({
    mutationFn: () =>
      api(`/pedidos/${p.id}`, {
        method: 'PATCH',
        body: JSON.stringify({
          cliente_id: f.cliente ? Number(f.cliente) : null,
          fecha_entrega: deFecha(f.entrega),
          descuento: aNumero(f.descuento) ?? 0,
          envio: aNumero(f.envio) ?? 0,
          notas: f.notas,
        }),
      }),
    onSuccess: refrescar,
  })
  return (
    <Stack gap="sm">
      <SimpleGrid cols={{ base: 1, sm: 2, md: 4 }}>
        <Select
          label="Cliente"
          searchable
          clearable
          data={(clientes.data ?? []).map((c) => ({ value: String(c.id), label: c.nombre }))}
          value={f.cliente}
          onChange={(v) => setF({ ...f, cliente: v })}
        />
        <TextInput label="Entregar el" type="date" value={f.entrega} onChange={(e) => setF({ ...f, entrega: e.currentTarget.value })} />
        <NumberInput label="Descuento" prefix="$" min={0} decimalScale={2} value={f.descuento} onChange={(v) => setF({ ...f, descuento: v })} />
        <NumberInput label="Envío" prefix="$" min={0} decimalScale={2} value={f.envio} onChange={(v) => setF({ ...f, envio: v })} />
      </SimpleGrid>
      <Textarea label="Notas" autosize minRows={1} value={f.notas} onChange={(e) => setF({ ...f, notas: e.currentTarget.value })} />
      {guardar.error && <Alert color="red">{guardar.error.message}</Alert>}
      {cambio && (
        <Group>
          <Button size="xs" loading={guardar.isPending} onClick={() => guardar.mutate()}>
            Guardar
          </Button>
          <Button size="xs" variant="subtle" onClick={() => setF(inicial())}>
            Deshacer cambios
          </Button>
        </Group>
      )}
    </Stack>
  )
}

function Linea({ p, l, abierto }: { p: PedidoFicha; l: LineaPedido; abierto: boolean }) {
  const refrescar = useRefrescarVentas()
  const [precio, setPrecio] = useState<number | string>(l.precio)
  const [cantidad, setCantidad] = useState<number | string>(l.cantidad)
  const editar = useMutation({
    mutationFn: () =>
      api(`/pedidos/${p.id}/piezas/${l.id}`, {
        method: 'PATCH',
        body: JSON.stringify({ precio: aNumero(precio), cantidad: aNumero(cantidad) }),
      }),
    onSuccess: refrescar,
  })
  const accion = useMutation({
    mutationFn: (x: { que: 'apartar' | 'desapartar' | 'quitar'; existencia?: number }) =>
      x.que === 'quitar'
        ? api(`/pedidos/${p.id}/piezas/${l.id}`, { method: 'DELETE' })
        : api(`/pedidos/${p.id}/piezas/${l.id}/${x.que}`, {
            method: 'POST',
            body: JSON.stringify(x.existencia ? { existencia_id: x.existencia } : {}),
          }),
    onSuccess: refrescar,
  })
  const cambio = aNumero(precio) !== l.precio || aNumero(cantidad) !== l.cantidad
  const disponibles = l.disponibles ?? []
  const mismas = disponibles.filter((d) => d.acabado === l.acabado)
  return (
    <Table.Tr>
      <Table.Td w={52}>
        <Miniatura archivoId={l.portada?.id ?? 0} clave={l.portada?.clave} lado={40} />
      </Table.Td>
      <Table.Td>
        <Anchor component={Link} to={`/catalogo/${l.modelo_id}`} size="sm">
          {titulo(l)}
        </Anchor>
        <Group gap={4}>
          <Badge size="xs" color={ACABADO[l.acabado].color} variant="light">
            {ACABADO[l.acabado].texto}
          </Badge>
          <Text size="xs" c="dimmed">
            {l.faccion}
          </Text>
        </Group>
      </Table.Td>
      <Table.Td>
        <Group gap={4} wrap="nowrap">
          <NumberInput size="xs" w={70} min={1} allowDecimal={false} disabled={!abierto} value={cantidad} onChange={setCantidad} />
          <Text size="xs">×</Text>
          <NumberInput size="xs" w={100} prefix="$" min={0} decimalScale={2} disabled={!abierto} value={precio} onChange={setPrecio} />
        </Group>
        {l.precio === 0 && (
          <Text size="xs" c="orange">
            sin precio
          </Text>
        )}
        {cambio && (
          <Button size="compact-xs" mt={4} loading={editar.isPending} onClick={() => editar.mutate()}>
            Guardar
          </Button>
        )}
        {editar.error && (
          <Text size="xs" c="red">
            {editar.error.message}
          </Text>
        )}
      </Table.Td>
      <Table.Td ta="right">{dinero(l.subtotal)}</Table.Td>
      <Table.Td>
        {l.faltan === 0 ? (
          <Text size="sm" c="green">
            apartadas
          </Text>
        ) : (
          <Stack gap={2}>
            <Text size="sm" c="orange">
              faltan {n(l.faltan)}
              {l.apartadas > 0 ? ` (${n(l.apartadas)} apartadas)` : ''}
            </Text>
            {abierto && mismas.length > 0 && (
              <Button size="compact-xs" variant="light" loading={accion.isPending} onClick={() => accion.mutate({ que: 'apartar' })}>
                Apartar de {mismas.map((d) => `${d.ubicacion || 'sin lugar'} (${d.cantidad})`).join(', ')}
              </Button>
            )}
            {abierto && disponibles.length > mismas.length && (
              <Menu>
                <Menu.Target>
                  <Button size="compact-xs" variant="subtle">
                    Apartar de otro acabado…
                  </Button>
                </Menu.Target>
                <Menu.Dropdown>
                  {disponibles
                    .filter((d) => d.acabado !== l.acabado)
                    .map((d) => (
                      <Menu.Item key={d.id} onClick={() => accion.mutate({ que: 'apartar', existencia: d.id })}>
                        {ACABADO[d.acabado].texto} · {d.ubicacion || 'sin lugar'} ({d.cantidad})
                      </Menu.Item>
                    ))}
                </Menu.Dropdown>
              </Menu>
            )}
            {abierto && disponibles.length === 0 && (
              <Anchor component={Link} to={`/placas?modelo_id=${l.modelo_id}`} size="xs">
                {l.placas > 0 ? `Imprimir: ver sus ${n(l.placas)} placas` : 'No hay impresas ni placas confirmadas'}
              </Anchor>
            )}
          </Stack>
        )}
        {accion.error && (
          <Text size="xs" c="red">
            {accion.error.message}
          </Text>
        )}
      </Table.Td>
      <Table.Td ta="right">
        {abierto && (
          <Menu position="bottom-end">
            <Menu.Target>
              <ActionIcon variant="subtle" color="gray" aria-label="Más">
                ⋯
              </ActionIcon>
            </Menu.Target>
            <Menu.Dropdown>
              {l.apartadas > 0 && (
                <Menu.Item onClick={() => accion.mutate({ que: 'desapartar' })}>Regresar las apartadas a las existencias</Menu.Item>
              )}
              <Menu.Item color="red" onClick={() => accion.mutate({ que: 'quitar' })}>
                Quitar del pedido
              </Menu.Item>
            </Menu.Dropdown>
          </Menu>
        )}
      </Table.Td>
    </Table.Tr>
  )
}

function AgregarPieza({ p }: { p: PedidoFicha }) {
  const refrescar = useRefrescarVentas()
  const [modelo, setModelo] = useState<string | null>(null)
  const [acabado, setAcabado] = useState<Acabado>('sin_pintar')
  const [cantidad, setCantidad] = useState<number | string>(1)
  const [precio, setPrecio] = useState<number | string>('')
  const agregar = useMutation({
    mutationFn: () =>
      api<{ sin_precio: boolean }>(`/pedidos/${p.id}/piezas`, {
        method: 'POST',
        body: JSON.stringify({ modelo_id: Number(modelo), acabado, cantidad: aNumero(cantidad), precio: aNumero(precio) }),
      }),
    onSuccess: () => {
      setModelo(null)
      setPrecio('')
      setCantidad(1)
      refrescar()
    },
  })
  return (
    <Group gap="xs" align="flex-end" wrap="wrap">
      <div style={{ flex: '1 1 240px' }}>
        <BuscarModelo label="Agregar un modelo" value={modelo} onChange={(v) => setModelo(v)} />
      </div>
      <Select
        label="Acabado"
        data={Object.entries(ACABADO).map(([value, a]) => ({ value, label: a.texto }))}
        value={acabado}
        allowDeselect={false}
        onChange={(v) => v && setAcabado(v as Acabado)}
        w={150}
      />
      <NumberInput label="Cuántas" min={1} allowDecimal={false} value={cantidad} onChange={setCantidad} w={90} />
      <NumberInput label="Precio c/u" placeholder="el sugerido" prefix="$" min={0} decimalScale={2} value={precio} onChange={setPrecio} w={130} />
      <Button disabled={!modelo} loading={agregar.isPending} onClick={() => agregar.mutate()}>
        Agregar
      </Button>
      {agregar.error && (
        <Text size="sm" c="red">
          {agregar.error.message}
        </Text>
      )}
    </Group>
  )
}

function Pagos({ p }: { p: PedidoFicha }) {
  const refrescar = useRefrescarVentas()
  const [monto, setMonto] = useState<number | string>(p.saldo > 0 ? p.saldo : '')
  const [metodo, setMetodo] = useState<string>('transferencia')
  const [notas, setNotas] = useState('')
  const pagar = useMutation({
    mutationFn: () => api(`/pedidos/${p.id}/pagos`, { method: 'POST', body: JSON.stringify({ monto: aNumero(monto), metodo, notas }) }),
    onSuccess: () => {
      setNotas('')
      refrescar()
    },
  })
  const borrar = useMutation({
    mutationFn: (id: number) => api(`/pagos/${id}`, { method: 'DELETE' }),
    onSuccess: refrescar,
  })
  return (
    <Stack gap="xs">
      {p.pagos.length > 0 && (
        <Table>
          <Table.Tbody>
            {p.pagos.map((x) => (
              <Table.Tr key={x.id}>
                <Table.Td>
                  <Text size="sm">{cuando(x.fecha)}</Text>
                </Table.Td>
                <Table.Td>
                  <Text size="sm">{METODO_PAGO[x.metodo]}</Text>
                  {x.notas && (
                    <Text size="xs" c="dimmed">
                      {x.notas}
                    </Text>
                  )}
                </Table.Td>
                <Table.Td ta="right">
                  <Text size="sm" c={x.monto < 0 ? 'red' : undefined}>
                    {dinero(x.monto)}
                  </Text>
                </Table.Td>
                <Table.Td ta="right">
                  <Button size="compact-xs" variant="subtle" color="gray" onClick={() => borrar.mutate(x.id)}>
                    Borrar
                  </Button>
                </Table.Td>
              </Table.Tr>
            ))}
          </Table.Tbody>
        </Table>
      )}
      {p.estado !== 'cancelado' && (
        <Group gap="xs" align="flex-end" wrap="wrap">
          <NumberInput label="Monto" description="En negativo si devuelves" prefix="$" decimalScale={2} value={monto} onChange={setMonto} w={150} />
          <Select
            label="Forma"
            data={Object.entries(METODO_PAGO).map(([value, label]) => ({ value, label }))}
            value={metodo}
            allowDeselect={false}
            onChange={(v) => v && setMetodo(v)}
            w={150}
          />
          <TextInput label="Nota" placeholder="anticipo, saldo…" value={notas} onChange={(e) => setNotas(e.currentTarget.value)} style={{ flex: 1 }} miw={120} />
          <Button disabled={!aNumero(monto)} loading={pagar.isPending} onClick={() => pagar.mutate()}>
            Registrar pago
          </Button>
        </Group>
      )}
      {(pagar.error ?? borrar.error) && <Alert color="red">{(pagar.error ?? borrar.error)?.message}</Alert>}
    </Stack>
  )
}

export default function Pedido() {
  const { id } = useParams()
  const navegar = useNavigate()
  const refrescar = useRefrescarVentas()
  const q = useQuery({ queryKey: ['pedido', id], queryFn: () => api<PedidoFicha>(`/pedidos/${id}`) })
  const borrar = useMutation({
    mutationFn: () => api(`/pedidos/${id}`, { method: 'DELETE' }),
    onSuccess: () => {
      refrescar()
      navegar('/ventas')
    },
  })
  if (q.isPending) return <Loader />
  if (q.error) return <Alert color="red">{q.error.message}</Alert>
  const p = q.data
  const abierto = p.estado !== 'entregado' && p.estado !== 'cancelado'
  const c = p.datos_cliente
  return (
    <Stack maw={1100}>
      <Breadcrumbs>
        <Anchor component={Link} to="/ventas">
          Pedidos
        </Anchor>
        <Text>#{p.id}</Text>
      </Breadcrumbs>
      <Group justify="space-between" align="flex-start">
        <div>
          <Title order={2}>
            Pedido #{p.id} {c ? `· ${c.nombre}` : ''}
          </Title>
          <Text size="sm" c="dimmed">
            {[c?.whatsapp && `WhatsApp ${c.whatsapp}`, c?.telefono, c?.correo, c?.redes].filter(Boolean).join(' · ')}
            {p.coleccion_id && (
              <>
                {c ? ' · ' : ''}de la colección{' '}
                <Anchor component={Link} to={`/ventas/colecciones/${p.coleccion_id}`} size="sm">
                  {p.coleccion}
                </Anchor>
              </>
            )}
          </Text>
          <Text size="xs" c="dimmed">
            Creado {cuando(p.creado)}
            {p.entregado ? ` · entregado ${cuando(p.entregado)}` : ''}
          </Text>
        </div>
        {(p.estado === 'cotizado' || p.estado === 'cancelado') && (
          <Button size="xs" variant="subtle" color="red" loading={borrar.isPending} onClick={() => borrar.mutate()}>
            Borrar pedido
          </Button>
        )}
      </Group>
      {borrar.error && <Alert color="red">{borrar.error.message}</Alert>}
      <Estado p={p} />
      <Card withBorder>
        <Datos key={p.actualizado} p={p} />
      </Card>
      <Card withBorder>
        <Text fw={500} mb="xs">
          Miniaturas
        </Text>
        {p.lineas.length > 0 && (
          <Table.ScrollContainer minWidth={760}>
            <Table verticalSpacing={6}>
              <Table.Tbody>
                {p.lineas.map((l) => (
                  <Linea key={`${l.id}-${l.cantidad}-${l.precio}-${l.apartadas}`} p={p} l={l} abierto={abierto} />
                ))}
              </Table.Tbody>
            </Table>
          </Table.ScrollContainer>
        )}
        {abierto && <AgregarPieza p={p} />}
      </Card>
      <SimpleGrid cols={{ base: 1, md: 2 }}>
        <Card withBorder>
          <Text fw={500} mb="xs">
            Cuentas
          </Text>
          <Table>
            <Table.Tbody>
              <Table.Tr>
                <Table.Td>Miniaturas ({n(p.piezas)})</Table.Td>
                <Table.Td ta="right">{dinero(p.subtotal)}</Table.Td>
              </Table.Tr>
              {p.descuento > 0 && (
                <Table.Tr>
                  <Table.Td>Descuento</Table.Td>
                  <Table.Td ta="right">−{dinero(p.descuento)}</Table.Td>
                </Table.Tr>
              )}
              {p.envio > 0 && (
                <Table.Tr>
                  <Table.Td>Envío</Table.Td>
                  <Table.Td ta="right">{dinero(p.envio)}</Table.Td>
                </Table.Tr>
              )}
              <Table.Tr>
                <Table.Td fw={700}>Total</Table.Td>
                <Table.Td ta="right" fw={700}>
                  {dinero(p.total)}
                </Table.Td>
              </Table.Tr>
              <Table.Tr>
                <Table.Td>Pagado</Table.Td>
                <Table.Td ta="right">{dinero(p.pagado)}</Table.Td>
              </Table.Tr>
              <Table.Tr>
                <Table.Td>Saldo</Table.Td>
                <Table.Td ta="right" c={p.saldo > 0 ? 'orange' : 'green'}>
                  {dinero(p.saldo)}
                </Table.Td>
              </Table.Tr>
              <Table.Tr>
                <Table.Td>
                  <Text size="sm">Ganancia</Text>
                  <Text size="xs" c="dimmed">
                    precio − descuento − costo de lo apartado
                  </Text>
                </Table.Td>
                <Table.Td ta="right">
                  {p.ganancia != null ? (
                    <Text size="sm" c="green">
                      {dinero(p.ganancia)}
                    </Text>
                  ) : (
                    <Text size="xs" c="dimmed">
                      {p.faltan > 0 ? 'cuando todo esté apartado' : 'sin costo'}
                    </Text>
                  )}
                </Table.Td>
              </Table.Tr>
            </Table.Tbody>
          </Table>
        </Card>
        <Card withBorder>
          <Text fw={500} mb="xs">
            Pagos
          </Text>
          <Pagos p={p} />
        </Card>
      </SimpleGrid>
      <Card withBorder>
        <Text fw={500} mb="xs">
          Impresiones para este pedido
        </Text>
        <ListaDeImpresiones filtro={`pedido_id=${p.id}`} porPagina={10} />
        <Text size="xs" c="dimmed" mt="xs">
          Desde la ficha de una placa, «Registrar impresión» deja elegir este pedido: lo que salga se aparta solo.
        </Text>
      </Card>
    </Stack>
  )
}
