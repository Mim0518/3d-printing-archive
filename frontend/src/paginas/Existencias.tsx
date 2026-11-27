import {
  Alert,
  Anchor,
  Autocomplete,
  Badge,
  Button,
  Card,
  Group,
  Loader,
  Modal,
  NumberInput,
  SegmentedControl,
  Select,
  SimpleGrid,
  Stack,
  Table,
  Tabs,
  Text,
  TextInput,
  Title,
  Tooltip,
} from '@mantine/core'
import { useDebouncedValue } from '@mantine/hooks'
import { keepPreviousData, useMutation, useQuery } from '@tanstack/react-query'
import { useState } from 'react'
import { Link, useSearchParams } from 'react-router'

import { api } from '../api'
import { ListaDeImpresiones } from '../componentes/Impresiones'
import Miniatura from '../componentes/Miniatura'
import { cuando, dinero, n, plural, titulo } from '../formato'
import { ACABADO, MOTIVO, aNumero, useOpciones, useRefrescarExistencias } from '../opciones'
import type { Acabado, AjustesCostos, Existencia, ExistenciaFicha, ListaModelos, PrecioSugerido, ResumenExistencias } from '../tipos'

/** «$10.00 de impresión + $110.00 de acabado × 3 + $5.00 de empaque». */
export function ExplicarPrecio({ p }: { p: PrecioSugerido }) {
  if (p.sugerido == null) {
    return (
      <Text size="xs" c="orange">
        Sin precio sugerido: falta {p.faltan.join(', ')}.
      </Text>
    )
  }
  return (
    <Text size="xs" c="dimmed">
      ({dinero(p.costo)} de impresión{p.acabado ? ` + ${dinero(p.acabado)} de acabado` : ''}) × {p.margen}
      {p.empaque ? ` + ${dinero(p.empaque)} de empaque` : ''}
      {p.faltan.length > 0 ? ` · sin contar ${p.faltan.join(', ')}` : ''}
    </Text>
  )
}

function Acciones({ e, ubicaciones }: { e: ExistenciaFicha; ubicaciones: string[] }) {
  const refrescar = useRefrescarExistencias()
  const [que, setQue] = useState<'mover' | 'merma' | 'ajuste'>('mover')
  const [cantidad, setCantidad] = useState<number | string>(1)
  const [acabado, setAcabado] = useState<Acabado>(e.acabado === 'sin_pintar' ? 'pintada' : e.acabado)
  const [ubicacion, setUbicacion] = useState(e.ubicacion)
  const [notas, setNotas] = useState('')
  const hacer = useMutation({
    mutationFn: () => {
      const c = aNumero(cantidad) ?? 0
      if (que === 'mover')
        return api(`/existencias/${e.id}/mover`, {
          method: 'POST',
          body: JSON.stringify({ cantidad: c, acabado, ubicacion, notas }),
        })
      return api(`/existencias/${e.id}/movimientos`, {
        method: 'POST',
        body: JSON.stringify({ delta: que === 'merma' ? -c : c, motivo: que, notas }),
      })
    },
    onSuccess: () => {
      setNotas('')
      refrescar()
    },
  })
  return (
    <Card withBorder padding="xs">
      <SegmentedControl
        size="xs"
        value={que}
        onChange={(v) => setQue(v as typeof que)}
        data={[
          { value: 'mover', label: 'Pintar o mover' },
          { value: 'merma', label: 'Se rompió o perdió' },
          { value: 'ajuste', label: 'Ajustar el conteo' },
        ]}
      />
      <Group gap="xs" align="flex-end" mt="xs" wrap="wrap">
        <NumberInput
          label={que === 'ajuste' ? 'Sumar (negativo resta)' : 'Cuántas'}
          min={que === 'ajuste' ? -10000 : 1}
          max={10000}
          allowDecimal={false}
          value={cantidad}
          onChange={setCantidad}
          w={130}
        />
        {que === 'mover' && (
          <>
            <Select
              label="Acabado"
              data={Object.entries(ACABADO).map(([value, a]) => ({ value, label: a.texto }))}
              value={acabado}
              allowDeselect={false}
              onChange={(v) => v && setAcabado(v as Acabado)}
              w={160}
            />
            <Autocomplete label="Lugar" data={ubicaciones} value={ubicacion} onChange={setUbicacion} w={160} />
          </>
        )}
        <TextInput label="Nota" value={notas} onChange={(ev) => setNotas(ev.currentTarget.value)} style={{ flex: 1, minWidth: 140 }} />
        <Button loading={hacer.isPending} onClick={() => hacer.mutate()}>
          Hacerlo
        </Button>
      </Group>
      {hacer.error && (
        <Text size="sm" c="red" mt={4}>
          {hacer.error.message}
        </Text>
      )}
    </Card>
  )
}

function Precio({ e }: { e: ExistenciaFicha }) {
  const refrescar = useRefrescarExistencias()
  const [precio, setPrecio] = useState<number | string>(e.precio ?? '')
  const [minimo, setMinimo] = useState<number | string>(e.minimo ?? '')
  const cambio = aNumero(precio) !== e.precio || aNumero(minimo) !== e.minimo
  const guardar = useMutation({
    mutationFn: () =>
      api(`/existencias/${e.id}`, {
        method: 'PATCH',
        body: JSON.stringify({ precio: aNumero(precio), minimo: aNumero(minimo) }),
      }),
    onSuccess: refrescar,
  })
  return (
    <Group gap="xs" align="flex-end" wrap="wrap">
      <NumberInput
        label="Precio de venta"
        description={e.precio_sugerido.sugerido != null ? `Sugerido: ${dinero(e.precio_sugerido.sugerido)}` : 'Vacío: el sugerido'}
        prefix="$"
        min={0}
        decimalScale={2}
        placeholder={e.precio_sugerido.sugerido != null ? String(e.precio_sugerido.sugerido) : ''}
        value={precio}
        onChange={setPrecio}
        w={170}
      />
      <NumberInput
        label="Mínimo"
        description="Avisa cuando hay menos"
        min={0}
        allowDecimal={false}
        value={minimo}
        onChange={setMinimo}
        w={140}
      />
      <Button size="sm" variant="light" disabled={!cambio} loading={guardar.isPending} onClick={() => guardar.mutate()}>
        Guardar
      </Button>
      {guardar.error && (
        <Text size="sm" c="red">
          {guardar.error.message}
        </Text>
      )}
    </Group>
  )
}

/** Una existencia: su precio, qué hacer con ella y su historial de movimientos. */
export function FichaExistencia({ id, ubicaciones }: { id: number; ubicaciones: string[] }) {
  const q = useQuery({ queryKey: ['existencia', id], queryFn: () => api<ExistenciaFicha>(`/existencias/${id}`) })
  if (q.isPending) return <Loader />
  if (q.error) return <Alert color="red">{q.error.message}</Alert>
  const e = q.data
  return (
    <Stack gap="sm">
      <Group gap="sm" wrap="nowrap" align="flex-start">
        <Miniatura archivoId={e.portada?.id ?? 0} clave={e.portada?.clave} lado={72} />
        <div>
          <Anchor component={Link} to={`/catalogo/${e.modelo_id}`} fw={600}>
            {titulo(e)}
          </Anchor>
          <Text size="sm" c="dimmed">
            {[e.juego, e.faccion, e.estudio].filter(Boolean).join(' · ')}
          </Text>
          <Group gap={4} mt={4}>
            <Badge color={ACABADO[e.acabado].color} variant="light">
              {ACABADO[e.acabado].texto}
            </Badge>
            <Badge variant="outline" color="gray">
              {e.ubicacion || 'sin lugar'}
            </Badge>
          </Group>
        </div>
      </Group>
      <Text>
        Hay <b>{n(e.cantidad)}</b> · cuestan {e.costo_unitario != null ? dinero(e.costo_unitario) : '¿?'} cada una
        {e.precio_venta != null && (
          <>
            {' '}
            · se venden a <b>{dinero(e.precio_venta)}</b>
            {e.precio == null ? ' (sugerido)' : ''}
          </>
        )}
      </Text>
      <ExplicarPrecio p={e.precio_sugerido} />
      <Precio key={`${e.precio}-${e.minimo}`} e={e} />
      <Acciones key={e.cantidad} e={e} ubicaciones={ubicaciones} />
      <Text fw={500} size="sm">
        Movimientos
      </Text>
      <Table.ScrollContainer minWidth={480}>
        <Table>
          <Table.Tbody>
            {e.movimientos.map((m) => (
              <Table.Tr key={m.id}>
                <Table.Td>
                  <Text size="xs" style={{ whiteSpace: 'nowrap' }}>
                    {cuando(m.fecha)}
                  </Text>
                </Table.Td>
                <Table.Td>
                  <Text size="sm" c={m.delta > 0 ? 'green' : 'red'} fw={600}>
                    {m.delta > 0 ? `+${m.delta}` : m.delta}
                  </Text>
                </Table.Td>
                <Table.Td>
                  <Text size="sm">
                    {MOTIVO[m.motivo]}
                    {m.par_acabado || m.par_ubicacion != null
                      ? ` ${m.delta > 0 ? 'desde' : 'a'} ${[m.par_acabado ? ACABADO[m.par_acabado].texto : '', m.par_ubicacion ?? '']
                          .filter(Boolean)
                          .join(', ')}`
                      : ''}
                  </Text>
                  {m.notas && (
                    <Text size="xs" c="dimmed">
                      {m.notas}
                    </Text>
                  )}
                </Table.Td>
                <Table.Td ta="right">
                  <Text size="xs" c="dimmed">
                    {m.costo_unitario != null ? `${dinero(m.costo_unitario)} c/u` : ''}
                  </Text>
                </Table.Td>
              </Table.Tr>
            ))}
          </Table.Tbody>
        </Table>
      </Table.ScrollContainer>
    </Stack>
  )
}

/** Miniaturas que ya tenías antes de la app. */
export function AgregarExistencia({ modelo, ubicaciones, onListo }: { modelo?: { id: number; nombre: string }; ubicaciones: string[]; onListo: () => void }) {
  const refrescar = useRefrescarExistencias()
  const [texto, setTexto] = useState('')
  const [buscado] = useDebouncedValue(texto, 300)
  const [modeloId, setModeloId] = useState<string | null>(modelo ? String(modelo.id) : null)
  const [acabado, setAcabado] = useState<Acabado>('sin_pintar')
  const [ubicacion, setUbicacion] = useState('')
  const [cantidad, setCantidad] = useState<number | string>(1)
  const [costo, setCosto] = useState<number | string>('')
  const encontrados = useQuery({
    queryKey: ['buscar-modelos', buscado],
    queryFn: () => api<ListaModelos>(`/modelos?q=${encodeURIComponent(buscado)}&por_pagina=20`),
    enabled: !modelo && buscado.trim().length > 1,
  })
  const crear = useMutation({
    mutationFn: () =>
      api('/existencias', {
        method: 'POST',
        body: JSON.stringify({
          modelo_id: Number(modeloId),
          acabado,
          ubicacion,
          cantidad: aNumero(cantidad),
          costo_unitario: aNumero(costo),
        }),
      }),
    onSuccess: () => {
      refrescar()
      onListo()
    },
  })
  return (
    <Stack gap="sm">
      {modelo ? (
        <Text fw={500}>{modelo.nombre}</Text>
      ) : (
        <Select
          label="Modelo"
          placeholder="Escribe su nombre…"
          searchable
          searchValue={texto}
          onSearchChange={setTexto}
          filter={({ options }) => options}
          data={(encontrados.data?.modelos ?? []).map((m) => ({
            value: String(m.id),
            label: `${titulo(m)}${m.faccion && m.nombre ? ` (${m.faccion})` : ''}`,
          }))}
          value={modeloId}
          onChange={setModeloId}
        />
      )}
      <SimpleGrid cols={{ base: 1, xs: 2 }}>
        <Select
          label="Acabado"
          data={Object.entries(ACABADO).map(([value, a]) => ({ value, label: a.texto }))}
          value={acabado}
          allowDeselect={false}
          onChange={(v) => v && setAcabado(v as Acabado)}
        />
        <Autocomplete label="Dónde están" placeholder="Caja 3…" data={ubicaciones} value={ubicacion} onChange={setUbicacion} />
        <NumberInput label="Cuántas" min={1} allowDecimal={false} value={cantidad} onChange={setCantidad} />
        <NumberInput
          label="Costo por miniatura (si lo sabes)"
          prefix="$"
          min={0}
          decimalScale={2}
          value={costo}
          onChange={setCosto}
        />
      </SimpleGrid>
      {crear.error && <Alert color="red">{crear.error.message}</Alert>}
      <Button disabled={!modeloId || !aNumero(cantidad)} loading={crear.isPending} onClick={() => crear.mutate()}>
        Agregar
      </Button>
    </Stack>
  )
}

function ExistenciasTab() {
  const [params, setParams] = useSearchParams()
  const { filtros } = useOpciones()
  const [texto, setTexto] = useState(params.get('q') ?? '')
  const [buscado] = useDebouncedValue(texto, 300)
  const [abierta, setAbierta] = useState<number | null>(null)
  const [agregar, setAgregar] = useState(false)
  const consulta = new URLSearchParams()
  if (buscado) consulta.set('q', buscado)
  for (const k of ['juego', 'acabado', 'ubicacion']) {
    const v = params.get(k)
    if (v) consulta.set(k, v)
  }
  // «ver» es la pestaña; el filtro de cuáles mostrar va en «mostrar»
  if (params.get('mostrar')) consulta.set('ver', params.get('mostrar')!)
  const poner = (k: string, v: string | null) => {
    const p = new URLSearchParams(params)
    if (v) p.set(k, v)
    else p.delete(k)
    setParams(p, { replace: true })
  }
  const lista = useQuery({
    queryKey: ['existencias', consulta.toString()],
    queryFn: () => api<Existencia[]>(`/existencias?${consulta}`),
    placeholderData: keepPreviousData,
  })
  const resumen = useQuery({ queryKey: ['existencias-resumen'], queryFn: () => api<ResumenExistencias>('/existencias/resumen') })
  const ubicaciones = (resumen.data?.ubicaciones ?? []).map((u) => u.ubicacion).filter(Boolean)
  const filas = lista.data ?? []
  return (
    <Stack>
      {resumen.data && resumen.data.ajustes_faltan.length > 0 && (
        <Alert color="orange" variant="light">
          <Text size="sm">
            Los costos y precios no están completos: falta {resumen.data.ajustes_faltan.join(', ')}.{' '}
            <Anchor size="sm" component={Link} to="/existencias?ver=costos">
              Captúralos en Costos
            </Anchor>
            .
          </Text>
        </Alert>
      )}
      <Group gap="xs" grow wrap="wrap">
        <TextInput placeholder="Buscar modelo, facción o lugar…" value={texto} onChange={(e) => setTexto(e.currentTarget.value)} miw={200} />
        <Select
          placeholder="Juego"
          clearable
          data={(filtros?.juegos ?? []).map((j) => j.nombre)}
          value={params.get('juego')}
          onChange={(v) => poner('juego', v)}
          miw={140}
        />
        <Select
          placeholder="Acabado"
          clearable
          data={Object.entries(ACABADO).map(([value, a]) => ({ value, label: a.texto }))}
          value={params.get('acabado')}
          onChange={(v) => poner('acabado', v)}
          miw={140}
        />
        <Select placeholder="Lugar" clearable data={ubicaciones} value={params.get('ubicacion')} onChange={(v) => poner('ubicacion', v)} miw={120} />
        <Select
          data={[
            { value: 'con', label: 'Las que hay' },
            { value: 'bajo_minimo', label: 'Bajo el mínimo' },
            { value: 'todas', label: 'Todas (también en cero)' },
          ]}
          value={params.get('mostrar') ?? 'con'}
          allowDeselect={false}
          onChange={(v) => poner('mostrar', v === 'con' ? null : v)}
          miw={150}
        />
      </Group>
      <Group justify="space-between">
        <Text size="sm" c="dimmed">
          {plural(
            filas.reduce((s, x) => s + x.cantidad, 0),
            'miniatura',
            'miniaturas',
          )}{' '}
          en {plural(filas.length, 'existencia', 'existencias')}
        </Text>
        <Button size="xs" variant="light" onClick={() => setAgregar(true)}>
          Agregar las que ya tenías
        </Button>
      </Group>
      {lista.isPending ? (
        <Loader />
      ) : filas.length === 0 ? (
        <Text c="dimmed" ta="center" py="lg">
          No hay existencias {params.toString() || buscado ? 'que coincidan' : 'todavía: registra una impresión desde una placa, o agrega las que ya tenías'}.
        </Text>
      ) : (
        <Table.ScrollContainer minWidth={760}>
          <Table highlightOnHover style={{ opacity: lista.isPlaceholderData ? 0.6 : 1 }}>
            <Table.Thead>
              <Table.Tr>
                <Table.Th />
                <Table.Th>Modelo</Table.Th>
                <Table.Th>Acabado y lugar</Table.Th>
                <Table.Th ta="right">Hay</Table.Th>
                <Table.Th ta="right">Costo c/u</Table.Th>
                <Table.Th ta="right">Precio c/u</Table.Th>
                <Table.Th ta="right">Valor</Table.Th>
              </Table.Tr>
            </Table.Thead>
            <Table.Tbody>
              {filas.map((e) => (
                <Table.Tr key={e.id} style={{ cursor: 'pointer' }} onClick={() => setAbierta(e.id)}>
                  <Table.Td w={52}>
                    <Miniatura archivoId={e.portada?.id ?? 0} clave={e.portada?.clave} lado={40} />
                  </Table.Td>
                  <Table.Td>
                    <Text size="sm">{titulo(e)}</Text>
                    <Text size="xs" c="dimmed">
                      {[e.juego, e.faccion].filter(Boolean).join(' · ')}
                    </Text>
                  </Table.Td>
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
                    {e.minimo != null && (
                      <Text size="xs" c="dimmed">
                        mín. {e.minimo}
                      </Text>
                    )}
                  </Table.Td>
                  <Table.Td ta="right">{e.costo_unitario != null ? dinero(e.costo_unitario) : '—'}</Table.Td>
                  <Table.Td ta="right">
                    {e.precio_venta != null ? (
                      <Tooltip label={e.precio == null ? 'Sugerido' : 'Puesto a mano'}>
                        <Text size="sm" c={e.precio == null ? 'dimmed' : undefined}>
                          {dinero(e.precio_venta)}
                        </Text>
                      </Tooltip>
                    ) : (
                      '—'
                    )}
                  </Table.Td>
                  <Table.Td ta="right">{e.valor_venta != null ? dinero(e.valor_venta) : '—'}</Table.Td>
                </Table.Tr>
              ))}
            </Table.Tbody>
          </Table>
        </Table.ScrollContainer>
      )}
      <Modal opened={abierta !== null} onClose={() => setAbierta(null)} title="Existencia" size="lg">
        {abierta !== null && <FichaExistencia id={abierta} ubicaciones={ubicaciones} />}
      </Modal>
      <Modal opened={agregar} onClose={() => setAgregar(false)} title="Agregar miniaturas que ya tenías">
        {agregar && <AgregarExistencia ubicaciones={ubicaciones} onListo={() => setAgregar(false)} />}
      </Modal>
    </Stack>
  )
}

function CostosTab() {
  const refrescar = useRefrescarExistencias()
  const q = useQuery({ queryKey: ['ajustes-costos'], queryFn: () => api<AjustesCostos>('/ajustes/costos') })
  const [valores, setValores] = useState<Record<string, number | string> | null>(null)
  const guardar = useMutation({
    mutationFn: (v: Record<string, number | string>) =>
      api('/ajustes/costos', {
        method: 'PUT',
        body: JSON.stringify({ valores: Object.fromEntries(Object.entries(v).map(([k, x]) => [k, aNumero(x)])) }),
      }),
    onSuccess: () => {
      void q.refetch()
      refrescar()
    },
  })
  if (q.isPending) return <Loader />
  if (q.error) return <Alert color="red">{q.error.message}</Alert>
  const d = q.data
  const v = valores ?? Object.fromEntries(d.campos.map((c) => [c.clave, d.valores[c.clave] ?? '']))
  const cambio = d.campos.some((c) => aNumero(v[c.clave]) !== (d.valores[c.clave] ?? null))
  return (
    <Stack maw={760}>
      <Text size="sm" c="dimmed">
        Costo de una impresión = material + luz + limpieza + fallas, repartido entre las miniaturas que salen bien. Precio
        sugerido = (costo + imprimación, pintura y mano de obra según el acabado) × margen + empaque, redondeado hacia arriba.
        Lo que dejes vacío no se cuenta. Las impresiones ya registradas guardan el costo con el que se registraron.
      </Text>
      <SimpleGrid cols={{ base: 1, sm: 2 }}>
        {d.campos.map((c) => (
          <NumberInput
            key={c.clave}
            label={c.etiqueta}
            description={c.ayuda}
            suffix={c.unidad.startsWith('$') ? undefined : ` ${c.unidad}`}
            prefix={c.unidad.startsWith('$') ? '$' : undefined}
            rightSection={c.unidad.startsWith('$/') ? <Text size="xs" c="dimmed">{c.unidad.slice(1)}</Text> : undefined}
            rightSectionWidth={c.unidad.startsWith('$/') ? 44 : undefined}
            min={c.min}
            max={c.max}
            decimalScale={2}
            value={v[c.clave]}
            onChange={(x) => setValores({ ...v, [c.clave]: x })}
          />
        ))}
      </SimpleGrid>
      {guardar.error && <Alert color="red">{guardar.error.message}</Alert>}
      <Group>
        <Button disabled={!cambio} loading={guardar.isPending} onClick={() => guardar.mutate(v)}>
          Guardar
        </Button>
        <Text size="sm" c="dimmed">
          Moneda: {d.moneda}
        </Text>
      </Group>
    </Stack>
  )
}

export default function Existencias() {
  const [params, setParams] = useSearchParams()
  const resumen = useQuery({ queryKey: ['existencias-resumen'], queryFn: () => api<ResumenExistencias>('/existencias/resumen') })
  const ver = params.get('ver') === 'impresiones' || params.get('ver') === 'costos' ? params.get('ver')! : 'existencias'
  const r = resumen.data
  return (
    <Stack maw={1200}>
      <div>
        <Title order={2}>Existencias</Title>
        {r && (
          <Text c="dimmed" size="sm">
            {plural(r.piezas, 'miniatura', 'miniaturas')} de {plural(r.modelos, 'modelo', 'modelos')} · valen{' '}
            {dinero(r.valor_costo)} a costo
            {r.valor_venta ? ` y ${dinero(r.valor_venta)} a precio` : ''}
            {r.bajo_minimo ? ` · ${n(r.bajo_minimo)} bajo el mínimo` : ''} ·{' '}
            {plural(r.impresiones_mes, 'impresión', 'impresiones')} en 30 días ({dinero(r.costo_mes)})
          </Text>
        )}
      </div>
      <Tabs
        value={ver}
        onChange={(v) => {
          const p = new URLSearchParams()
          if (v && v !== 'existencias') p.set('ver', v)
          setParams(p, { replace: true })
        }}
      >
        <Tabs.List>
          <Tabs.Tab value="existencias">Existencias</Tabs.Tab>
          <Tabs.Tab value="impresiones">Impresiones</Tabs.Tab>
          <Tabs.Tab value="costos">Costos</Tabs.Tab>
        </Tabs.List>
        <Tabs.Panel value="existencias" pt="md">
          {ver === 'existencias' && <ExistenciasTab />}
        </Tabs.Panel>
        <Tabs.Panel value="impresiones" pt="md">
          {ver === 'impresiones' && <ListaDeImpresiones />}
        </Tabs.Panel>
        <Tabs.Panel value="costos" pt="md">
          {ver === 'costos' && <CostosTab />}
        </Tabs.Panel>
      </Tabs>
    </Stack>
  )
}
