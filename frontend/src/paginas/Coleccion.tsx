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
  Modal,
  NumberInput,
  Select,
  SimpleGrid,
  Stack,
  Table,
  Text,
  TextInput,
  Textarea,
  Title,
  UnstyledButton,
} from '@mantine/core'
import { useMutation, useQuery } from '@tanstack/react-query'
import { useState } from 'react'
import { Link, useNavigate, useParams } from 'react-router'

import { api } from '../api'
import BuscarModelo from '../componentes/BuscarModelo'
import Miniatura from '../componentes/Miniatura'
import { dinero, n, plural, titulo } from '../formato'
import { compartirODescargar, imagenColeccion } from '../imagenColeccion'
import { ACABADO, ESTADO_COLECCION, aNumero, useRefrescarVentas } from '../opciones'
import type { Acabado, Cliente, ColeccionFicha, FotoModelo, LineaColeccion } from '../tipos'

function Datos({ c }: { c: ColeccionFicha }) {
  const refrescar = useRefrescarVentas()
  const inicial = () => ({
    nombre: c.nombre,
    descripcion: c.descripcion,
    estado: c.estado as string,
    precio_fijo: (c.precio_fijo ?? '') as number | string,
  })
  const [f, setF] = useState(inicial)
  const cambio =
    f.nombre !== c.nombre || f.descripcion !== c.descripcion || f.estado !== c.estado || aNumero(f.precio_fijo) !== c.precio_fijo
  const guardar = useMutation({
    mutationFn: () =>
      api(`/colecciones/${c.id}`, {
        method: 'PATCH',
        body: JSON.stringify({ ...f, precio_fijo: aNumero(f.precio_fijo) }),
      }),
    onSuccess: refrescar,
  })
  return (
    <Stack gap="sm">
      <SimpleGrid cols={{ base: 1, sm: 3 }}>
        <TextInput label="Nombre" value={f.nombre} onChange={(e) => setF({ ...f, nombre: e.currentTarget.value })} />
        <Select
          label="Estado"
          data={Object.entries(ESTADO_COLECCION).map(([value, e]) => ({ value, label: e.texto }))}
          value={f.estado}
          allowDeselect={false}
          onChange={(v) => v && setF({ ...f, estado: v })}
        />
        <NumberInput
          label="Precio fijo"
          description={`Vacío: la suma (${dinero(c.suma)})`}
          prefix="$"
          min={0}
          decimalScale={2}
          value={f.precio_fijo}
          onChange={(v) => setF({ ...f, precio_fijo: v })}
        />
      </SimpleGrid>
      <Textarea
        label="Descripción"
        placeholder="Lo que verá quien la reciba: qué incluye, escala, si viene armada…"
        autosize
        minRows={2}
        value={f.descripcion}
        onChange={(e) => setF({ ...f, descripcion: e.currentTarget.value })}
      />
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

function ElegirFoto({ c, l, onListo }: { c: ColeccionFicha; l: LineaColeccion; onListo: () => void }) {
  const refrescar = useRefrescarVentas()
  const q = useQuery({ queryKey: ['fotos', l.modelo_id], queryFn: () => api<FotoModelo[]>(`/modelos/${l.modelo_id}/fotos`) })
  const poner = useMutation({
    mutationFn: (foto: number | null) =>
      api(`/colecciones/${c.id}/modelos/${l.id}`, { method: 'PATCH', body: JSON.stringify({ foto_id: foto }) }),
    onSuccess: () => {
      refrescar()
      onListo()
    },
  })
  if (q.isPending) return <Loader />
  return (
    <Stack gap="xs">
      <Text size="sm" c="dimmed">
        Imágenes, renders y mallas de {titulo(l)} con miniatura. Para usar fotos tuyas de la miniatura pintada, ponlas en la
        carpeta del modelo y escanea.
      </Text>
      <SimpleGrid cols={{ base: 3, sm: 5 }} spacing="xs">
        {(q.data ?? []).map((f) => (
          <UnstyledButton
            key={f.id}
            onClick={() => poner.mutate(f.id)}
            style={{ outline: l.foto?.id === f.id ? '3px solid var(--mantine-color-blue-6)' : undefined, borderRadius: 4 }}
            title={f.nombre}
          >
            <Miniatura archivoId={f.id} clave={f.clave} tamano={128} lado={110} lleno />
          </UnstyledButton>
        ))}
      </SimpleGrid>
      {q.data?.length === 0 && (
        <Text size="sm" c="orange">
          Este modelo todavía no tiene imágenes con miniatura.
        </Text>
      )}
      {l.foto_id && (
        <Button variant="subtle" size="xs" onClick={() => poner.mutate(null)}>
          Usar la portada del modelo
        </Button>
      )}
    </Stack>
  )
}

function Linea({ c, l, ultima, primera }: { c: ColeccionFicha; l: LineaColeccion; primera: boolean; ultima: boolean }) {
  const refrescar = useRefrescarVentas()
  const [foto, setFoto] = useState(false)
  const [cantidad, setCantidad] = useState<number | string>(l.cantidad)
  const [precio, setPrecio] = useState<number | string>(l.precio_a_mano ?? '')
  const editar = useMutation({
    mutationFn: (x: Record<string, unknown>) =>
      api(`/colecciones/${c.id}/modelos/${l.id}`, { method: 'PATCH', body: JSON.stringify(x) }),
    onSuccess: refrescar,
  })
  const quitar = useMutation({
    mutationFn: () => api(`/colecciones/${c.id}/modelos/${l.id}`, { method: 'DELETE' }),
    onSuccess: refrescar,
  })
  const mover = (d: number) => {
    const otra = c.lineas[c.lineas.findIndex((x) => x.id === l.id) + d]
    if (!otra) return
    // se intercambian sus lugares
    void api(`/colecciones/${c.id}/modelos/${otra.id}`, { method: 'PATCH', body: JSON.stringify({ orden: l.orden }) }).then(() =>
      editar.mutate({ orden: otra.orden === l.orden ? l.orden + d : otra.orden }),
    )
  }
  const cambio = aNumero(cantidad) !== l.cantidad || aNumero(precio) !== l.precio_a_mano
  return (
    <Table.Tr>
      <Table.Td w={76}>
        <UnstyledButton onClick={() => setFoto(true)} title="Cambiar la foto">
          <Miniatura archivoId={l.foto?.id ?? 0} clave={l.foto?.clave} lado={64} />
        </UnstyledButton>
      </Table.Td>
      <Table.Td>
        <Anchor component={Link} to={`/catalogo/${l.modelo_id}`} size="sm">
          {titulo(l)}
        </Anchor>
        <Text size="xs" c="dimmed">
          {[l.juego, l.faccion].filter(Boolean).join(' · ')}
        </Text>
        <Select
          size="xs"
          mt={4}
          w={160}
          data={Object.entries(ACABADO).map(([value, a]) => ({ value, label: a.texto }))}
          value={l.acabado}
          allowDeselect={false}
          onChange={(v) => v && editar.mutate({ acabado: v })}
        />
      </Table.Td>
      <Table.Td>
        <Group gap={4} wrap="nowrap">
          <NumberInput size="xs" w={64} min={1} allowDecimal={false} value={cantidad} onChange={setCantidad} />
          <Text size="xs">×</Text>
          <NumberInput
            size="xs"
            w={110}
            prefix="$"
            min={0}
            decimalScale={2}
            placeholder={l.precio_sugerido != null ? String(l.precio_sugerido) : 'sin precio'}
            value={precio}
            onChange={setPrecio}
          />
        </Group>
        <Text size="xs" c="dimmed">
          {l.precio_a_mano != null
            ? 'precio a mano'
            : l.precio_sugerido != null
              ? 'precio de venta o sugerido'
              : 'sin precio: captura costos o pon uno'}
        </Text>
        {cambio && (
          <Button
            size="compact-xs"
            mt={4}
            loading={editar.isPending}
            onClick={() => editar.mutate({ cantidad: aNumero(cantidad), precio: aNumero(precio) })}
          >
            Guardar
          </Button>
        )}
        {editar.error && (
          <Text size="xs" c="red">
            {editar.error.message}
          </Text>
        )}
      </Table.Td>
      <Table.Td ta="right">{l.subtotal != null ? dinero(l.subtotal) : '—'}</Table.Td>
      <Table.Td>
        {l.faltan === 0 ? (
          <Text size="xs" c="green">
            hay {n(l.hay)}
          </Text>
        ) : (
          <Stack gap={0}>
            <Text size="xs" c="orange">
              faltan {n(l.faltan)} (hay {n(l.hay)})
            </Text>
            <Anchor component={Link} to={`/placas?modelo_id=${l.modelo_id}`} size="xs">
              {l.placas ? `${plural(l.placas, 'placa', 'placas')}` : 'sin placas'}
            </Anchor>
          </Stack>
        )}
      </Table.Td>
      <Table.Td>
        <Group gap={2} wrap="nowrap">
          <ActionIcon variant="subtle" color="gray" disabled={primera} onClick={() => mover(-1)} aria-label="Subir">
            ↑
          </ActionIcon>
          <ActionIcon variant="subtle" color="gray" disabled={ultima} onClick={() => mover(1)} aria-label="Bajar">
            ↓
          </ActionIcon>
          <ActionIcon variant="subtle" color="red" loading={quitar.isPending} onClick={() => quitar.mutate()} aria-label="Quitar">
            ×
          </ActionIcon>
        </Group>
      </Table.Td>
      <Modal opened={foto} onClose={() => setFoto(false)} title="Foto para la colección" size="lg">
        {foto && <ElegirFoto c={c} l={l} onListo={() => setFoto(false)} />}
      </Modal>
    </Table.Tr>
  )
}

function Agregar({ c }: { c: ColeccionFicha }) {
  const refrescar = useRefrescarVentas()
  const [modelo, setModelo] = useState<string | null>(null)
  const [acabado, setAcabado] = useState<Acabado>('sin_pintar')
  const [cantidad, setCantidad] = useState<number | string>(1)
  const agregar = useMutation({
    mutationFn: () =>
      api(`/colecciones/${c.id}/modelos`, {
        method: 'POST',
        body: JSON.stringify({ modelo_id: Number(modelo), acabado, cantidad: aNumero(cantidad) }),
      }),
    onSuccess: () => {
      setModelo(null)
      setCantidad(1)
      refrescar()
    },
  })
  return (
    <Group gap="xs" align="flex-end" wrap="wrap" mt="sm">
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

function APedido({ c }: { c: ColeccionFicha }) {
  const navegar = useNavigate()
  const refrescar = useRefrescarVentas()
  const clientes = useQuery({ queryKey: ['clientes', ''], queryFn: () => api<Cliente[]>('/clientes') })
  const [cliente, setCliente] = useState<string | null>(null)
  const [nuevo, setNuevo] = useState('')
  const [veces, setVeces] = useState<number | string>(1)
  const crear = useMutation({
    mutationFn: () =>
      api<{ id: number }>(`/colecciones/${c.id}/pedido`, {
        method: 'POST',
        body: JSON.stringify({
          cliente_id: cliente ? Number(cliente) : null,
          cliente: !cliente && nuevo.trim() ? { nombre: nuevo } : null,
          veces: aNumero(veces) ?? 1,
        }),
      }),
    onSuccess: (r) => {
      refrescar()
      navegar(`/ventas/pedidos/${r.id}`)
    },
  })
  return (
    <Stack gap="sm">
      <Select
        label="Cliente"
        placeholder="Elige uno, o escribe abajo uno nuevo"
        searchable
        clearable
        data={(clientes.data ?? []).map((x) => ({ value: String(x.id), label: x.nombre }))}
        value={cliente}
        onChange={setCliente}
      />
      {!cliente && <TextInput label="…o un cliente nuevo" placeholder="Nombre" value={nuevo} onChange={(e) => setNuevo(e.currentTarget.value)} />}
      <NumberInput
        label="Cuántas colecciones"
        min={1}
        allowDecimal={false}
        value={veces}
        onChange={setVeces}
        description={`Total: ${dinero(c.precio * (aNumero(veces) ?? 1))}`}
      />
      {crear.error && <Alert color="red">{crear.error.message}</Alert>}
      <Button loading={crear.isPending} onClick={() => crear.mutate()}>
        Crear el pedido (cotizado)
      </Button>
    </Stack>
  )
}

export default function Coleccion() {
  const { id } = useParams()
  const navegar = useNavigate()
  const refrescar = useRefrescarVentas()
  const [pedido, setPedido] = useState(false)
  const [imagen, setImagen] = useState<string | null>(null)
  const q = useQuery({ queryKey: ['coleccion', id], queryFn: () => api<ColeccionFicha>(`/colecciones/${id}`) })
  const exportar = useMutation({
    mutationFn: async (c: ColeccionFicha) => compartirODescargar(await imagenColeccion(c), c.nombre),
    onSuccess: (r) => setImagen(r === 'compartida' ? 'Imagen compartida.' : 'Imagen descargada: mándala por WhatsApp o tus redes.'),
  })
  const borrar = useMutation({
    mutationFn: () => api(`/colecciones/${id}`, { method: 'DELETE' }),
    onSuccess: () => {
      refrescar()
      navegar('/ventas?ver=colecciones')
    },
  })
  if (q.isPending) return <Loader />
  if (q.error) return <Alert color="red">{q.error.message}</Alert>
  const c = q.data
  return (
    <Stack maw={1100}>
      <Breadcrumbs>
        <Anchor component={Link} to="/ventas?ver=colecciones">
          Colecciones
        </Anchor>
        <Text>{c.nombre}</Text>
      </Breadcrumbs>
      <Group justify="space-between" align="flex-start">
        <div>
          <Group gap="xs">
            <Title order={2}>{c.nombre}</Title>
            <Badge color={ESTADO_COLECCION[c.estado].color} variant="light">
              {ESTADO_COLECCION[c.estado].texto}
            </Badge>
          </Group>
          <Text size="lg" fw={600}>
            {dinero(c.precio)}{' '}
            <Text span size="sm" c="dimmed" fw={400}>
              {c.precio_fijo != null ? `precio fijo (la suma es ${dinero(c.suma)})` : 'la suma de sus modelos'} ·{' '}
              {plural(c.piezas, 'miniatura', 'miniaturas')}
            </Text>
          </Text>
          <Text size="sm" c="dimmed">
            {c.costo != null ? `Cuesta ${dinero(c.costo)} · ganancia ${dinero(c.ganancia ?? 0)}` : 'Sin el costo de todos sus modelos'}
            {c.sin_precio ? ` · ${plural(c.sin_precio, 'modelo', 'modelos')} sin precio` : ''}
          </Text>
          <Text size="sm" c={c.alcanza ? 'green' : 'orange'}>
            {c.lineas.length === 0 ? '' : c.alcanza ? 'Alcanzan las existencias' : `Faltan ${n(c.faltan)} miniaturas por imprimir`}
          </Text>
        </div>
        <Group gap="xs">
          <Button
            variant="light"
            disabled={c.lineas.length === 0}
            component="a"
            href={`/hoja/coleccion/${c.id}`}
            target="_blank"
          >
            Hoja para PDF
          </Button>
          <Button variant="light" disabled={c.lineas.length === 0} loading={exportar.isPending} onClick={() => exportar.mutate(c)}>
            Imagen para WhatsApp
          </Button>
          <Button disabled={c.lineas.length === 0} onClick={() => setPedido(true)}>
            Convertir en pedido
          </Button>
        </Group>
      </Group>
      {imagen && (
        <Alert color="green" withCloseButton onClose={() => setImagen(null)}>
          {imagen}
        </Alert>
      )}
      {exportar.error && <Alert color="red">{exportar.error.message}</Alert>}
      <Card withBorder>
        <Datos key={c.actualizada} c={c} />
      </Card>
      <Card withBorder>
        <Text fw={500} mb="xs">
          Modelos
        </Text>
        {c.lineas.length > 0 && (
          <Table.ScrollContainer minWidth={760}>
            <Table verticalSpacing={6}>
              <Table.Tbody>
                {c.lineas.map((l, i) => (
                  <Linea
                    key={`${l.id}-${l.cantidad}-${l.precio_a_mano}-${l.orden}`}
                    c={c}
                    l={l}
                    primera={i === 0}
                    ultima={i === c.lineas.length - 1}
                  />
                ))}
              </Table.Tbody>
            </Table>
          </Table.ScrollContainer>
        )}
        <Agregar c={c} />
      </Card>
      <Group>
        <Button variant="subtle" color="red" size="xs" loading={borrar.isPending} onClick={() => borrar.mutate()}>
          Borrar la colección
        </Button>
        <Text size="xs" c="dimmed">
          Los pedidos que salieron de ella se quedan.
        </Text>
      </Group>
      <Modal opened={pedido} onClose={() => setPedido(false)} title="Convertir en pedido">
        {pedido && <APedido c={c} />}
      </Modal>
    </Stack>
  )
}
