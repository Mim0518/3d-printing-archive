import {
  Alert,
  Anchor,
  Badge,
  Button,
  Card,
  Checkbox,
  Group,
  Loader,
  Modal,
  SegmentedControl,
  SimpleGrid,
  Stack,
  Switch,
  Table,
  Tabs,
  Text,
  TextInput,
  Title,
} from '@mantine/core'
import { useMutation, useQuery } from '@tanstack/react-query'
import { useEffect, useState } from 'react'
import { Link, useSearchParams } from 'react-router'

import { api } from '../api'
import { ResumenResultado } from '../componentes/FrioDelModelo'
import { cuando, n, plural, tamano } from '../formato'
import { useRefrescarFrio } from '../opciones'
import { useTrabajo } from '../trabajo'
import type { LotePapelera, ResumenFrio, Transferencia, UbicacionFrio } from '../tipos'

function TarjetaUbicacion({ u }: { u: UbicacionFrio }) {
  const refrescar = useRefrescarFrio()
  const [trabajo, setTrabajo] = useState<number | null>(null)
  const t = useTrabajo(trabajo)
  const estado = t.data?.estado
  useEffect(() => {
    if (estado && estado !== 'en_curso') refrescar()
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [estado])
  const probar = useMutation({ mutationFn: () => api<{ ok: boolean; error?: string }>(`/frio/ubicaciones/${u.id}/probar`, { method: 'POST' }) })
  const listar = useMutation({
    mutationFn: () => api<{ trabajo_id: number }>(`/frio/ubicaciones/${u.id}/listar`, { method: 'POST' }),
    onSuccess: (r) => setTrabajo(r.trabajo_id),
  })
  const cambiar = useMutation({
    mutationFn: (x: Record<string, unknown>) => api(`/frio/ubicaciones/${u.id}`, { method: 'PATCH', body: JSON.stringify(x) }),
    onSuccess: refrescar,
  })
  const r = t.data?.resultado as { archivos?: number; nuevos?: number; ya_no_estan?: number; ligados?: number } | null
  return (
    <Card withBorder>
      <Group justify="space-between" align="flex-start" wrap="nowrap">
        <div style={{ minWidth: 0 }}>
          <Group gap="xs">
            <Text fw={600}>{u.nombre}</Text>
            <Badge size="xs" variant="light" color={u.tipo === 'drive' ? 'blue' : 'grape'}>
              {u.tipo === 'drive' ? 'rclone' : 'disco externo'}
            </Badge>
            {u.predeterminada && (
              <Badge size="xs" variant="outline">
                predeterminada
              </Badge>
            )}
          </Group>
          <Text size="xs" c="dimmed" ff="monospace" style={{ wordBreak: 'break-all' }}>
            {u.tipo === 'drive' ? u.remoto : u.ruta_base}
          </Text>
        </div>
      </Group>
      <Text size="sm" mt="xs">
        {plural(u.copias, 'copia', 'copias')} · {tamano(u.bytes)} · {plural(u.modelos, 'modelo', 'modelos')}
      </Text>
      <Text size="xs" c="dimmed">
        {n(u.verificadas)} verificadas por MD5
        {u.faltan ? ` · ${n(u.faltan)} ya no están` : ''} · último listado: {cuando(u.ultimo_listado)}
      </Text>
      <Group gap="xs" mt="sm">
        <Switch
          size="xs"
          label="Recibe copias nuevas"
          checked={u.escribible}
          onChange={(e) => cambiar.mutate({ escribible: e.currentTarget.checked })}
        />
        {!u.predeterminada && u.escribible && (
          <Button size="compact-xs" variant="subtle" onClick={() => cambiar.mutate({ predeterminada: true })}>
            Hacerla la predeterminada
          </Button>
        )}
      </Group>
      <Group gap="xs" mt="xs">
        <Button size="xs" variant="default" loading={probar.isPending} onClick={() => probar.mutate()}>
          Probar
        </Button>
        <Button
          size="xs"
          variant="light"
          loading={listar.isPending || estado === 'en_curso'}
          onClick={() => listar.mutate()}
          title="Lee todo lo que hay ahí con su MD5 y pone al día las copias (lo nuevo se liga a su modelo)"
        >
          Volver a listar
        </Button>
      </Group>
      {probar.data && (
        <Text size="xs" mt={4} c={probar.data.ok ? 'green' : 'red'}>
          {probar.data.ok ? 'Responde bien.' : probar.data.error}
        </Text>
      )}
      {(probar.error ?? listar.error ?? cambiar.error) && (
        <Text size="xs" c="red" mt={4}>
          {(probar.error ?? listar.error ?? cambiar.error)?.message}
        </Text>
      )}
      {estado === 'terminado' && r && (
        <Text size="xs" mt={4}>
          Listado: {n(r.archivos ?? 0)} archivos · {n(r.nuevos ?? 0)} nuevos ({n(r.ligados ?? 0)} ligados a un modelo) ·{' '}
          {n(r.ya_no_estan ?? 0)} ya no están
        </Text>
      )}
      {estado === 'fallo' && (
        <Text size="xs" c="red" mt={4}>
          {t.data?.detalle}
        </Text>
      )}
    </Card>
  )
}

function NuevaUbicacion({ onListo }: { onListo: () => void }) {
  const refrescar = useRefrescarFrio()
  const [tipo, setTipo] = useState<'disco_externo' | 'drive'>('disco_externo')
  const [nombre, setNombre] = useState('')
  const [ruta, setRuta] = useState('')
  const [remoto, setRemoto] = useState('')
  const [marca, setMarca] = useState(false)
  const crear = useMutation({
    mutationFn: () =>
      api('/frio/ubicaciones', {
        method: 'POST',
        body: JSON.stringify({ nombre, tipo, ruta_base: ruta, remoto, crear_marca: marca }),
      }),
    onSuccess: () => {
      refrescar()
      onListo()
    },
  })
  return (
    <Stack gap="sm">
      <SegmentedControl
        size="xs"
        value={tipo}
        onChange={(v) => setTipo(v as typeof tipo)}
        data={[
          { value: 'disco_externo', label: 'Disco externo' },
          { value: 'drive', label: 'Remoto de rclone' },
        ]}
      />
      <TextInput label="Nombre" placeholder={tipo === 'drive' ? 'Drive 2' : 'Disco de respaldo'} value={nombre} onChange={(e) => setNombre(e.currentTarget.value)} />
      {tipo === 'disco_externo' ? (
        <>
          <TextInput
            label="Dónde se monta en el mini PC"
            placeholder="/media/archivo/respaldo"
            value={ruta}
            onChange={(e) => setRuta(e.currentTarget.value)}
          />
          <Checkbox
            checked={marca}
            onChange={(e) => setMarca(e.currentTarget.checked)}
            label="Escribir en su raíz el archivo .archivo-3d-ubicacion"
            description="Así la app reconoce que es este disco y no otro montado en el mismo lugar. Solo se escribe ese archivo."
          />
        </>
      ) : (
        <TextInput
          label="Remoto"
          description="Un remoto que ya configuraste con rclone config en el mini PC; la app no ve sus credenciales."
          placeholder="DriveMemo:3DP-otro"
          value={remoto}
          onChange={(e) => setRemoto(e.currentTarget.value)}
        />
      )}
      {crear.error && <Alert color="red">{crear.error.message}</Alert>}
      <Button disabled={!nombre.trim()} loading={crear.isPending} onClick={() => crear.mutate()}>
        Agregar
      </Button>
    </Stack>
  )
}

function UbicacionesTab() {
  const q = useQuery({ queryKey: ['frio-ubicaciones'], queryFn: () => api<UbicacionFrio[]>('/frio/ubicaciones') })
  const [nueva, setNueva] = useState(false)
  if (q.isPending) return <Loader />
  if (q.error) return <Alert color="red">{q.error.message}</Alert>
  return (
    <Stack>
      <Text size="sm" c="dimmed">
        Para mandar un modelo a frío o traerlo, ve a su ficha en el catálogo. Lo nuevo se guarda como carpeta tal cual, con la
        misma ruta de la colección.
      </Text>
      <SimpleGrid cols={{ base: 1, md: 2 }}>
        {q.data.map((u) => (
          <TarjetaUbicacion key={u.id} u={u} />
        ))}
      </SimpleGrid>
      <Group>
        <Button variant="light" onClick={() => setNueva(true)}>
          Agregar una ubicación
        </Button>
      </Group>
      <Modal opened={nueva} onClose={() => setNueva(false)} title="Nueva ubicación">
        {nueva && <NuevaUbicacion onListo={() => setNueva(false)} />}
      </Modal>
    </Stack>
  )
}

function TransferenciasTab() {
  const q = useQuery({ queryKey: ['transferencias'], queryFn: () => api<Transferencia[]>('/frio/transferencias') })
  if (q.isPending) return <Loader />
  if (q.error) return <Alert color="red">{q.error.message}</Alert>
  if (q.data.length === 0)
    return (
      <Text c="dimmed" ta="center" py="lg">
        Todavía no se ha mandado ni traído nada desde la app.
      </Text>
    )
  return (
    <Table.ScrollContainer minWidth={720}>
      <Table verticalSpacing={6}>
        <Table.Tbody>
          {q.data.map((t) => (
            <Table.Tr key={t.id}>
              <Table.Td>
                <Text size="sm" style={{ whiteSpace: 'nowrap' }}>
                  {cuando(t.creada)}
                </Text>
                <Badge
                  size="xs"
                  variant="light"
                  color={t.estado === 'terminada' ? 'green' : t.estado === 'en_curso' ? 'blue' : t.estado === 'fallo' ? 'red' : 'orange'}
                >
                  {t.estado.replace('_', ' ')}
                </Badge>
              </Table.Td>
              <Table.Td>
                {t.modelo_id ? (
                  <Anchor component={Link} to={`/catalogo/${t.modelo_id}`} size="sm">
                    {t.modelo}
                  </Anchor>
                ) : (
                  '—'
                )}
                <Text size="xs" c="dimmed">
                  {t.direccion === 'subir' ? `→ ${t.ubicacion}` : `← ${t.ubicacion}`}
                  {t.quitar_local ? ' · y quitar del disco' : ''} · {plural(t.archivos, 'archivo', 'archivos')} ({tamano(t.bytes)})
                </Text>
              </Table.Td>
              <Table.Td>
                <ResumenResultado r={t.resultado} />
              </Table.Td>
            </Table.Tr>
          ))}
        </Table.Tbody>
      </Table>
    </Table.ScrollContainer>
  )
}

function Vaciar({ l, frase, onListo }: { l: LotePapelera; frase: string; onListo: () => void }) {
  const refrescar = useRefrescarFrio()
  const [escrito, setEscrito] = useState('')
  const vaciar = useMutation({
    mutationFn: () => api(`/papelera/${l.id}/vaciar`, { method: 'POST', body: JSON.stringify({ frase: escrito }) }),
    onSuccess: () => {
      refrescar()
      onListo()
    },
  })
  return (
    <Stack gap="sm">
      <Alert color="red" variant="light" title="Esto no se puede deshacer">
        <Text size="sm">
          Se borran para siempre del disco {plural(l.archivos, 'archivo', 'archivos')} ({tamano(l.bytes)}) de{' '}
          {l.modelo ?? 'este lote'}. Antes se vuelve a comprobar que cada uno siga en {l.ubicacion ?? 'frío'} con su mismo MD5;
          si falta uno, no se borra nada.
        </Text>
        {l.compartidos > 0 && (
          <Text size="sm" mt={4}>
            De eso, {tamano(l.compartidos)} siguen en el disco porque otro modelo comparte esos archivos (enlace duro).
          </Text>
        )}
      </Alert>
      <TextInput label={`Escribe «${frase}» para confirmar`} value={escrito} onChange={(e) => setEscrito(e.currentTarget.value)} />
      {vaciar.error && <Alert color="red">{vaciar.error.message}</Alert>}
      <Button color="red" disabled={escrito.trim() !== frase} loading={vaciar.isPending} onClick={() => vaciar.mutate()}>
        Borrar para siempre
      </Button>
    </Stack>
  )
}

function PapeleraTab() {
  const refrescar = useRefrescarFrio()
  const [todos, setTodos] = useState(false)
  const [vaciando, setVaciando] = useState<LotePapelera | null>(null)
  const q = useQuery({
    queryKey: ['papelera', todos],
    queryFn: () => api<{ lotes: LotePapelera[]; frase_vaciar: string }>(`/papelera${todos ? '?ver=todos' : ''}`),
  })
  const restaurar = useMutation({
    mutationFn: (id: number) => api<{ restaurados: number; conflictos: string[] }>(`/papelera/${id}/restaurar`, { method: 'POST' }),
    onSuccess: refrescar,
  })
  if (q.isPending) return <Loader />
  if (q.error) return <Alert color="red">{q.error.message}</Alert>
  return (
    <Stack>
      <Group justify="space-between">
        <Text size="sm" c="dimmed">
          Lo que se quitó del disco espera aquí (en la misma unidad, dentro de .archivo-papelera) hasta que lo restaures o lo
          vacíes.
        </Text>
        <Switch size="xs" label="Ver también los cerrados" checked={todos} onChange={(e) => setTodos(e.currentTarget.checked)} />
      </Group>
      {restaurar.data && (
        <Alert color={restaurar.data.conflictos.length ? 'orange' : 'green'}>
          {plural(restaurar.data.restaurados, 'archivo regresó', 'archivos regresaron')} a su lugar.
          {restaurar.data.conflictos.map((c) => (
            <Text key={c} size="xs">
              {c}
            </Text>
          ))}
        </Alert>
      )}
      {restaurar.error && <Alert color="red">{restaurar.error.message}</Alert>}
      {q.data.lotes.length === 0 ? (
        <Text c="dimmed" ta="center" py="lg">
          La papelera está vacía.
        </Text>
      ) : (
        <Table.ScrollContainer minWidth={680}>
          <Table verticalSpacing={6}>
            <Table.Tbody>
              {q.data.lotes.map((l) => (
                <Table.Tr key={l.id} style={{ opacity: l.estado === 'en_papelera' ? 1 : 0.55 }}>
                  <Table.Td>
                    <Text size="sm">{cuando(l.creado)}</Text>
                    <Badge size="xs" variant="light" color={l.estado === 'en_papelera' ? 'orange' : l.estado === 'vaciado' ? 'red' : 'green'}>
                      {l.estado === 'en_papelera' ? 'en la papelera' : l.estado}
                    </Badge>
                  </Table.Td>
                  <Table.Td>
                    {l.modelo_id ? (
                      <Anchor component={Link} to={`/catalogo/${l.modelo_id}`} size="sm">
                        {l.modelo}
                      </Anchor>
                    ) : (
                      <Text size="sm">—</Text>
                    )}
                    <Text size="xs" c="dimmed">
                      {l.motivo}
                    </Text>
                  </Table.Td>
                  <Table.Td ta="right">
                    <Text size="sm">{plural(l.archivos, 'archivo', 'archivos')}</Text>
                    <Text size="xs" c="dimmed">
                      {tamano(l.bytes)}
                      {l.compartidos ? ` (${tamano(l.compartidos)} compartidos)` : ''}
                    </Text>
                  </Table.Td>
                  <Table.Td ta="right">
                    {l.estado === 'en_papelera' && (
                      <Group gap={4} justify="flex-end" wrap="nowrap">
                        <Button
                          size="compact-xs"
                          variant="light"
                          loading={restaurar.isPending && restaurar.variables === l.id}
                          onClick={() => restaurar.mutate(l.id)}
                        >
                          Restaurar
                        </Button>
                        <Button size="compact-xs" variant="subtle" color="red" onClick={() => setVaciando(l)}>
                          Vaciar…
                        </Button>
                      </Group>
                    )}
                  </Table.Td>
                </Table.Tr>
              ))}
            </Table.Tbody>
          </Table>
        </Table.ScrollContainer>
      )}
      <Modal opened={vaciando !== null} onClose={() => setVaciando(null)} title="Vaciar de la papelera">
        {vaciando && <Vaciar l={vaciando} frase={q.data.frase_vaciar} onListo={() => setVaciando(null)} />}
      </Modal>
    </Stack>
  )
}

export default function Frio() {
  const [params, setParams] = useSearchParams()
  const resumen = useQuery({ queryKey: ['frio-resumen'], queryFn: () => api<ResumenFrio>('/frio/resumen') })
  const ver = ['transferencias', 'papelera'].includes(params.get('ver') ?? '') ? params.get('ver')! : 'ubicaciones'
  const r = resumen.data
  return (
    <Stack maw={1100}>
      <div>
        <Title order={2}>Almacenamiento frío</Title>
        {r && (
          <Text size="sm" c="dimmed">
            {plural(r.modelos_solo_en_frio, 'modelo solo en frío', 'modelos solo en frío')} ·{' '}
            {plural(r.modelos_en_los_dos, 'modelo en el disco y en frío', 'modelos en el disco y en frío')} · papelera:{' '}
            {plural(r.papelera.archivos, 'archivo', 'archivos')} ({tamano(r.papelera.bytes)})
          </Text>
        )}
        {r?.en_curso && (
          <Text size="sm" c="blue">
            En curso: {r.en_curso.detalle}
          </Text>
        )}
      </div>
      <Tabs value={ver} onChange={(v) => setParams(v && v !== 'ubicaciones' ? { ver: v } : {}, { replace: true })}>
        <Tabs.List>
          <Tabs.Tab value="ubicaciones">Ubicaciones</Tabs.Tab>
          <Tabs.Tab value="transferencias">Transferencias</Tabs.Tab>
          <Tabs.Tab value="papelera">Papelera {r && r.papelera.lotes > 0 ? `(${r.papelera.lotes})` : ''}</Tabs.Tab>
        </Tabs.List>
        <Tabs.Panel value="ubicaciones" pt="md">
          {ver === 'ubicaciones' && <UbicacionesTab />}
        </Tabs.Panel>
        <Tabs.Panel value="transferencias" pt="md">
          {ver === 'transferencias' && <TransferenciasTab />}
        </Tabs.Panel>
        <Tabs.Panel value="papelera" pt="md">
          {ver === 'papelera' && <PapeleraTab />}
        </Tabs.Panel>
      </Tabs>
    </Stack>
  )
}
