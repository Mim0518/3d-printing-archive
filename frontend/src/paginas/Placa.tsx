import {
  ActionIcon,
  Alert,
  Anchor,
  Badge,
  Breadcrumbs,
  Button,
  Card,
  Checkbox,
  Code,
  Group,
  Loader,
  Modal,
  NumberInput,
  ScrollArea,
  SegmentedControl,
  Select,
  Stack,
  Switch,
  Table,
  Text,
  TextInput,
  Textarea,
  Title,
  Tooltip,
} from '@mantine/core'
import { useDebouncedValue } from '@mantine/hooks'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { useState } from 'react'
import { Link, useNavigate, useParams } from 'react-router'

import { api } from '../api'
import Miniatura from '../componentes/Miniatura'
import { COLOR_TIPO, corto, cuando, duracion, maquina, tamano, titulo } from '../formato'
import { ESTADO_PLACA, nombreMaterial, useRefrescarTaller, useTaller } from '../opciones'
import type { ArchivoDePlaca, DatosArchivoPlaca, ListaModelos, ModeloDePlaca, PlacaFicha } from '../tipos'

/** Una línea con lo que se leyó de un laminado. */
function resumenDatos(d: DatosArchivoPlaca): string {
  const exp = d.exposicion_s
    ? `${corto(d.exposicion_s)} s${d.exposicion_base_s ? ` (base ${corto(d.exposicion_base_s)} s${d.capas_base ? ` ×${d.capas_base}` : ''})` : ''}`
    : ''
  return [
    maquina(d),
    d.capa_mm ? `capa ${corto(d.capa_mm)} mm` : '',
    exp,
    d.capas ? `${d.capas} capas` : '',
    d.alto_mm ? `${corto(d.alto_mm)} mm de alto` : '',
    duracion(d.tiempo_s),
    d.volumen_ml ? `${corto(d.volumen_ml)} ml` : '',
    d.peso_g ? `${corto(d.peso_g)} g` : '',
    d.n_placas && d.n_placas > 1 ? `${d.n_placas} placas en el proyecto` : '',
  ]
    .filter(Boolean)
    .join(' · ')
}

function relativa(ruta: string, carpeta: string): string {
  return carpeta && ruta.startsWith(carpeta + '/') ? ruta.slice(carpeta.length + 1) : ruta
}

function useRefrescar(id: string | undefined) {
  const qc = useQueryClient()
  return () => {
    void qc.invalidateQueries({ queryKey: ['placa', id] })
    void qc.invalidateQueries({ queryKey: ['placas'] })
    void qc.invalidateQueries({ queryKey: ['placas-resumen'] })
    void qc.invalidateQueries({ queryKey: ['placas-modelo'] })
  }
}

/** Con qué impresora, perfil y material se imprime: lo elegido a mano y lo que se reconoce de sus laminados. */
function ConQue({ p }: { p: PlacaFicha }) {
  const { impresoras, materiales, perfiles } = useTaller()
  const refrescarPlaca = useRefrescar(String(p.id))
  const refrescarTaller = useRefrescarTaller()
  const inicial = () => ({
    impresora: p.impresora_id ? String(p.impresora_id) : null,
    perfil: p.perfil_id ? String(p.perfil_id) : null,
    material: p.material_id ? String(p.material_id) : null,
  })
  const [f, setF] = useState(inicial)
  const cambio =
    f.impresora !== (p.impresora_id ? String(p.impresora_id) : null) ||
    f.perfil !== (p.perfil_id ? String(p.perfil_id) : null) ||
    f.material !== (p.material_id ? String(p.material_id) : null)
  const guardar = useMutation({
    mutationFn: () =>
      api(`/placas/${p.id}`, {
        method: 'PATCH',
        body: JSON.stringify({
          impresora_id: f.impresora ? Number(f.impresora) : null,
          perfil_id: f.perfil ? Number(f.perfil) : null,
          material_id: f.material ? Number(f.material) : null,
        }),
      }),
    onSuccess: () => {
      refrescarPlaca()
      refrescarTaller()
    },
  })
  // lo que dicen sus laminados, aunque se haya elegido a mano la misma impresora
  const reconocidas = p.archivos
    .map((a) => (a.rol === 'laminado' ? a.datos?.impresora : null))
    .filter((i, k, todas): i is { id: number; nombre: string } => !!i && todas.findIndex((x) => x?.id === i.id) === k)
  const perfilesLam = p.archivos
    .map((a) => (a.rol === 'laminado' ? a.datos?.perfil : null))
    .filter((x, k, todos): x is { id: number; nombre: string } => !!x && todos.findIndex((y) => y?.id === x.id) === k)
  const imp = impresoras.find((i) => String(i.id) === f.impresora)
  const deImpresoras = f.impresora ? [Number(f.impresora)] : reconocidas.map((i) => i.id)
  const opcionesPerfil = perfiles
    .filter((x) => deImpresoras.length === 0 || deImpresoras.includes(x.impresora_id))
    .map((x) => ({ value: String(x.id), label: `${x.nombre} (${x.impresora})` }))
  const tipo = imp ? (imp.tecnologia === 'fdm' ? 'filamento' : 'resina') : null
  if (impresoras.length === 0) {
    return (
      <Card withBorder>
        <Text fw={500}>Con qué se imprime</Text>
        <Text size="sm" c="dimmed">
          Agrega tus impresoras en{' '}
          <Anchor component={Link} to="/impresoras">
            Impresoras y materiales
          </Anchor>{' '}
          para elegir aquí con cuál, con qué perfil y con qué material se imprime.
        </Text>
      </Card>
    )
  }
  return (
    <Card withBorder>
      <Text fw={500}>Con qué se imprime</Text>
      <Text size="xs" c="dimmed" mb="xs">
        {reconocidas.length > 0 ? (
          <>
            Por sus laminados: {reconocidas.map((i) => i.nombre).join(', ')}
            {perfilesLam.length === 1 ? ` · perfil «${perfilesLam[0].nombre}»` : ''}
            {p.material?.por === 'perfil' ? ` · ${p.material.nombre}` : ''}.{' '}
            {p.impresora_id || p.perfil_id || p.material_id ? 'Lo elegido a mano manda.' : 'Elige a mano solo si es otra cosa.'}
          </>
        ) : p.impresora_id || p.perfil_id || p.material_id ? (
          'Elegido a mano.'
        ) : (
          'Sus laminados no dicen para qué impresora son (o no tiene): elígela a mano.'
        )}
      </Text>
      <Group grow align="flex-start">
        <Select
          label="Impresora"
          placeholder={reconocidas[0]?.nombre ?? 'Sin elegir'}
          clearable
          data={impresoras.map((i) => ({ value: String(i.id), label: i.nombre }))}
          value={f.impresora}
          onChange={(v) => {
            const otra = impresoras.find((i) => String(i.id) === v)
            const perfil = perfiles.find((x) => String(x.id) === f.perfil)
            const material = materiales.find((m) => String(m.id) === f.material)
            const tipoNuevo = otra ? (otra.tecnologia === 'fdm' ? 'filamento' : 'resina') : null
            setF({
              impresora: v,
              perfil: perfil && v && String(perfil.impresora_id) !== v ? null : f.perfil,
              material: material && tipoNuevo && material.tipo !== tipoNuevo ? null : f.material,
            })
          }}
        />
        <Select
          label="Perfil"
          placeholder={p.perfil?.por === 'laminado' ? p.perfil.nombre : 'Sin elegir'}
          clearable
          searchable
          data={opcionesPerfil}
          value={f.perfil}
          onChange={(v) => {
            const x = perfiles.find((y) => String(y.id) === v)
            setF({
              impresora: x ? String(x.impresora_id) : f.impresora,
              perfil: v,
              material: f.material ?? (x?.material_id ? String(x.material_id) : null),
            })
          }}
        />
        <Select
          label="Material"
          placeholder={p.material?.por === 'perfil' ? p.material.nombre : 'Sin elegir'}
          clearable
          searchable
          data={materiales.filter((m) => !tipo || m.tipo === tipo).map((m) => ({ value: String(m.id), label: nombreMaterial(m) }))}
          value={f.material}
          onChange={(v) => setF({ ...f, material: v })}
        />
      </Group>
      {guardar.error && (
        <Alert color="red" mt="xs">
          {guardar.error.message}
        </Alert>
      )}
      {cambio && (
        <Group mt="xs">
          <Button size="xs" loading={guardar.isPending} onClick={() => guardar.mutate()}>
            Guardar
          </Button>
          <Button size="xs" variant="subtle" onClick={() => setF(inicial())}>
            Deshacer cambios
          </Button>
        </Group>
      )}
    </Card>
  )
}

function Datos({ p }: { p: PlacaFicha }) {
  const refrescar = useRefrescar(String(p.id))
  const [f, setF] = useState({ nombre: p.nombre, estado: p.estado, favorita: p.favorita, sin_modelo: p.sin_modelo, notas: p.notas })
  const cambio =
    f.nombre !== p.nombre || f.estado !== p.estado || f.favorita !== p.favorita || f.sin_modelo !== p.sin_modelo || f.notas !== p.notas
  const guardar = useMutation({
    mutationFn: () => api(`/placas/${p.id}`, { method: 'PATCH', body: JSON.stringify(f) }),
    onSuccess: refrescar,
  })
  return (
    <Card withBorder>
      <Stack gap="sm">
        <TextInput label="Nombre" value={f.nombre} onChange={(e) => setF({ ...f, nombre: e.currentTarget.value })} />
        <div>
          <Text size="sm" fw={500} mb={4}>
            Estado
          </Text>
          <SegmentedControl
            size="xs"
            value={f.estado}
            onChange={(v) => setF({ ...f, estado: v as PlacaFicha['estado'] })}
            data={[
              { value: 'sin_probar', label: 'Sin probar' },
              { value: 'probada', label: 'Probada' },
              { value: 'fallida', label: 'Fallida' },
            ]}
          />
        </div>
        <Group>
          <Switch label="Favorita" checked={f.favorita} onChange={(e) => setF({ ...f, favorita: e.currentTarget.checked })} />
          <Switch
            label="No es de ningún modelo (prueba, calibración)"
            checked={f.sin_modelo}
            onChange={(e) => setF({ ...f, sin_modelo: e.currentTarget.checked })}
          />
        </Group>
        <Textarea
          label="Notas"
          placeholder="Ajustes que funcionaron, qué falló, para qué resina…"
          autosize
          minRows={2}
          value={f.notas}
          onChange={(e) => setF({ ...f, notas: e.currentTarget.value })}
        />
        {guardar.error && <Alert color="red">{guardar.error.message}</Alert>}
        <Group>
          <Button size="xs" disabled={!cambio} loading={guardar.isPending} onClick={() => guardar.mutate()}>
            Guardar
          </Button>
          {cambio && (
            <Button
              size="xs"
              variant="subtle"
              onClick={() => setF({ nombre: p.nombre, estado: p.estado, favorita: p.favorita, sin_modelo: p.sin_modelo, notas: p.notas })}
            >
              Deshacer cambios
            </Button>
          )}
        </Group>
      </Stack>
    </Card>
  )
}

function FilaModelo({ placa, m }: { placa: number; m: ModeloDePlaca }) {
  const refrescar = useRefrescar(String(placa))
  const poner = useMutation({
    mutationFn: (cuerpo: { estado?: string; cantidad?: number | null }) =>
      api(`/placas/${placa}/modelos/${m.id}`, { method: 'PUT', body: JSON.stringify(cuerpo) }),
    onSuccess: refrescar,
  })
  const quitar = useMutation({
    mutationFn: () => api(`/placas/${placa}/modelos/${m.id}`, { method: 'DELETE' }),
    onSuccess: refrescar,
  })
  const [cantidad, setCantidad] = useState<string | number>(m.cantidad ?? '')
  const guardarCantidad = () => {
    const c = cantidad === '' ? null : Number(cantidad)
    if (c !== m.cantidad) poner.mutate({ cantidad: c })
  }
  const rechazado = m.estado === 'rechazado'
  return (
    <Table.Tr style={{ opacity: rechazado ? 0.55 : 1 }}>
      <Table.Td>
        <Anchor component={Link} to={`/catalogo/${m.id}`} size="sm" fw={500} td={rechazado ? 'line-through' : undefined}>
          {titulo(m)}
          {m.estudio && ` (${m.estudio})`}
        </Anchor>
        <Text size="xs" c="dimmed">
          {[m.juego, m.faccion].filter(Boolean).join(' · ')}
        </Text>
      </Table.Td>
      <Table.Td>
        <Badge size="sm" variant="light" color={m.estado === 'confirmado' ? 'green' : rechazado ? 'red' : 'gray'}>
          {m.estado}
        </Badge>
        <Text size="xs" c="dimmed">
          {m.origen === 'carpeta' ? 'por la carpeta' : m.origen === 'nombre' ? 'por el nombre' : 'a mano'}
        </Text>
      </Table.Td>
      <Table.Td>
        {!rechazado && (
          <Tooltip label="Miniaturas completas de este modelo en la placa; vacío si la placa solo trae partes">
            <NumberInput
              size="xs"
              w={90}
              min={1}
              max={1000}
              placeholder="partes"
              value={cantidad}
              onChange={setCantidad}
              onBlur={guardarCantidad}
            />
          </Tooltip>
        )}
      </Table.Td>
      <Table.Td>
        <Group gap={4} wrap="nowrap">
          {m.estado !== 'confirmado' && (
            <Button size="compact-xs" variant="light" color="green" loading={poner.isPending} onClick={() => poner.mutate({ estado: 'confirmado' })}>
              Confirmar
            </Button>
          )}
          {!rechazado && (
            <Button size="compact-xs" variant="subtle" color="red" loading={quitar.isPending} onClick={() => quitar.mutate()}>
              {m.origen === 'manual' ? 'Quitar' : 'No es de este'}
            </Button>
          )}
        </Group>
        {(poner.error || quitar.error) && (
          <Text size="xs" c="red">
            {(poner.error ?? quitar.error)?.message}
          </Text>
        )}
      </Table.Td>
    </Table.Tr>
  )
}

function Modelos({ p }: { p: PlacaFicha }) {
  const refrescar = useRefrescar(String(p.id))
  const [busqueda, setBusqueda] = useState('')
  const [buscado] = useDebouncedValue(busqueda, 250)
  const opciones = useQuery({
    queryKey: ['modelos', 'buscar', buscado],
    queryFn: () => api<ListaModelos>(`/modelos?q=${encodeURIComponent(buscado)}&por_pagina=20`),
    enabled: buscado.trim().length >= 2,
  })
  const agregar = useMutation({
    mutationFn: (modelo: string) =>
      api(`/placas/${p.id}/modelos/${modelo}`, { method: 'PUT', body: JSON.stringify({ estado: 'confirmado' }) }),
    onSuccess: () => {
      setBusqueda('')
      refrescar()
    },
  })
  const yaEstan = new Set(p.modelos.map((m) => m.id))
  return (
    <Card withBorder>
      <Text fw={500}>De qué modelos es</Text>
      <Text size="xs" c="dimmed" mb="xs">
        Confirma las sugerencias o agrega otro modelo. Lo que marques como «no es de este» no se vuelve a sugerir.
      </Text>
      {p.modelos.length === 0 ? (
        <Text size="sm" c="dimmed" mb="xs">
          {p.sin_modelo ? 'Marcada como «no es de ningún modelo».' : 'Sin sugerencias: está fuera de las carpetas de los modelos.'}
        </Text>
      ) : (
        <ScrollArea>
          <Table miw={560}>
            <Table.Tbody>
              {p.modelos.map((m) => (
                <FilaModelo key={`${m.id}-${m.estado}-${m.cantidad}`} placa={p.id} m={m} />
              ))}
            </Table.Tbody>
          </Table>
        </ScrollArea>
      )}
      <Select
        mt="xs"
        placeholder="Agregar un modelo: escribe su nombre…"
        searchable
        searchValue={busqueda}
        onSearchChange={setBusqueda}
        filter={({ options }) => options}
        nothingFoundMessage={buscado.trim().length >= 2 && !opciones.isFetching ? 'No hay modelos con ese nombre' : undefined}
        data={(opciones.data?.modelos ?? [])
          .filter((m) => !yaEstan.has(m.id))
          .map((m) => ({ value: String(m.id), label: `${titulo(m)}${m.estudio ? ` (${m.estudio})` : ''} · ${m.faccion || m.juego}` }))}
        value={null}
        onChange={(v) => v && agregar.mutate(v)}
        disabled={agregar.isPending}
      />
      {agregar.error && (
        <Text size="xs" c="red" mt={4}>
          {agregar.error.message}
        </Text>
      )}
    </Card>
  )
}

function Archivos({ p, ver }: { p: PlacaFicha; ver: (a: { id: number; nombre: string; clave: string }) => void }) {
  const refrescar = useRefrescar(String(p.id))
  const navegar = useNavigate()
  const [sel, setSel] = useState<Set<number>>(new Set())
  const [nombre, setNombre] = useState<string | null>(null)
  const portada = useMutation({
    mutationFn: (archivo_id: number | null) =>
      api(`/placas/${p.id}/portada`, { method: 'PUT', body: JSON.stringify({ archivo_id }) }),
    onSuccess: refrescar,
  })
  const separar = useMutation({
    mutationFn: () =>
      api<{ id: number }>('/placas', {
        method: 'POST',
        body: JSON.stringify({
          nombre,
          archivo_ids: [...sel],
          modelo_ids: p.modelos.filter((m) => m.estado === 'confirmado').map((m) => m.id),
        }),
      }),
    onSuccess: (r) => {
      setNombre(null)
      setSel(new Set())
      refrescar()
      navegar(`/placas/${r.id}`)
    },
  })
  const estrella = (id: number) => {
    const es = p.portada?.id === id
    return (
      <Tooltip label={es ? (p.portada_elegida === id ? 'Portada elegida (quitar)' : 'Portada automática') : 'Usar como portada'}>
        <ActionIcon
          size="sm"
          variant={es ? 'filled' : 'default'}
          color="yellow"
          aria-label="Portada"
          loading={portada.isPending && portada.variables === id}
          onClick={() => portada.mutate(es && p.portada_elegida === id ? null : id)}
        >
          ★
        </ActionIcon>
      </Tooltip>
    )
  }
  const alternar = (id: number) =>
    setSel((s) => {
      const x = new Set(s)
      if (x.has(id)) x.delete(id)
      else x.add(id)
      return x
    })
  const filaArchivo = (a: ArchivoDePlaca) => (
    <Table.Tr key={a.id} c={a.estado === 'falta' ? 'red' : undefined}>
      <Table.Td>
        <Checkbox aria-label="Seleccionar" checked={sel.has(a.id)} onChange={() => alternar(a.id)} disabled={p.archivos.length < 2} />
      </Table.Td>
      <Table.Td>
        {a.miniatura ? (
          <button
            type="button"
            onClick={() => ver({ id: a.id, nombre: a.nombre, clave: a.miniatura! })}
            style={{ border: 0, padding: 0, background: 'none', cursor: 'zoom-in' }}
          >
            <Miniatura archivoId={a.id} clave={a.miniatura} lado={40} />
          </button>
        ) : (
          <Miniatura archivoId={a.id} clave={null} lado={40} tipo={a.tipo} ext={a.ext} />
        )}
      </Table.Td>
      <Table.Td>
        <Text size="sm" style={{ wordBreak: 'break-all' }}>
          {relativa(a.ruta, p.carpeta)}
        </Text>
        {a.datos && resumenDatos(a.datos) && (
          <Text size="xs" c="dimmed">
            {resumenDatos(a.datos)}
          </Text>
        )}
        {a.rol === 'laminado' && a.datos && !a.datos.error && (a.datos.res_x || a.datos.impresora) && (
          <Text size="xs" c={a.datos.impresora ? 'teal' : 'dimmed'}>
            {a.datos.impresora
              ? `Para ${a.datos.impresora.nombre}${a.datos.perfil ? ` · perfil «${a.datos.perfil.nombre}»` : ' · ningún perfil suyo tiene esta capa y exposición'}`
              : 'Para una impresora que no está en la app'}
          </Text>
        )}
        {a.datos?.error && (
          <Text size="xs" c="orange">
            No se pudo leer: {a.datos.error.split(': ').slice(1).join(': ') || a.datos.error}
          </Text>
        )}
        <Group gap={4}>
          {a.estado === 'falta' && (
            <Badge size="xs" color="red" variant="light">
              ya no está
            </Badge>
          )}
          {!a.auto && (
            <Badge size="xs" color="grape" variant="outline">
              puesto a mano
            </Badge>
          )}
          {a.rol === 'autoguardado' && (
            <Text size="xs" c="dimmed">
              {cuando(Math.round(a.mtime_ms / 1000))}
            </Text>
          )}
        </Group>
      </Table.Td>
      <Table.Td>
        <Badge size="xs" variant="light" color={a.rol === 'laminado' ? 'teal' : a.rol === 'escena' ? 'cyan' : 'gray'}>
          {a.rol}
        </Badge>
        <Badge size="xs" variant="outline" color={COLOR_TIPO[a.tipo] ?? 'gray'} ml={4}>
          {a.ext}
        </Badge>
      </Table.Td>
      <Table.Td ta="right">
        <Text size="sm">{tamano(a.bytes)}</Text>
      </Table.Td>
      <Table.Td>
        <Group gap={4} wrap="nowrap">
          {a.miniatura && estrella(a.id)}
          {a.estado === 'presente' && (
            <Button size="compact-xs" variant="default" component="a" href={`/api/v1/archivos/${a.id}/contenido`} download={a.nombre}>
              Descargar
            </Button>
          )}
        </Group>
      </Table.Td>
    </Table.Tr>
  )

  return (
    <Card withBorder>
      <Group justify="space-between" mb="xs">
        <div>
          <Text fw={500}>Archivos</Text>
          <Text size="xs" c="dimmed">
            La escena, sus laminados y los autoguardados de Lychee. Las vistas previas salen de los mismos archivos.
          </Text>
        </div>
        {sel.size > 0 && sel.size < p.archivos.length && (
          <Button size="xs" variant="light" onClick={() => setNombre(p.nombre)}>
            Separar {sel.size} en una placa nueva
          </Button>
        )}
      </Group>
      <ScrollArea>
        <Table striped miw={720}>
          <Table.Tbody>{p.archivos.map(filaArchivo)}</Table.Tbody>
        </Table>
      </ScrollArea>
      {p.gemela && (
        <Group mt="sm" gap="sm" wrap="nowrap">
          {p.gemela.miniatura ? (
            <Miniatura archivoId={p.gemela.id} clave={p.gemela.miniatura} lado={40} />
          ) : (
            <Miniatura archivoId={p.gemela.id} clave={null} lado={40} tipo="malla" ext="stl" />
          )}
          <Text size="xs" c="dimmed" style={{ flex: 1, wordBreak: 'break-all' }}>
            Malla con el mismo nombre (se usa como vista previa si la escena no trae una): {relativa(p.gemela.ruta, p.carpeta)}
          </Text>
          {p.gemela.miniatura && estrella(p.gemela.id)}
        </Group>
      )}
      {portada.error && (
        <Text size="xs" c="red" mt={4}>
          {portada.error.message}
        </Text>
      )}
      <Modal opened={nombre !== null} onClose={() => setNombre(null)} title="Placa nueva" centered>
        <Stack>
          <Text size="sm">
            Los {sel.size} archivos elegidos pasan a una placa aparte, hecha a mano (la detección ya no los mueve). Lleva
            los modelos confirmados de esta.
          </Text>
          <TextInput label="Nombre" value={nombre ?? ''} onChange={(e) => setNombre(e.currentTarget.value)} />
          {separar.error && <Alert color="red">{separar.error.message}</Alert>}
          <Button loading={separar.isPending} disabled={!nombre?.trim()} onClick={() => separar.mutate()}>
            Crear la placa
          </Button>
        </Stack>
      </Modal>
    </Card>
  )
}

export default function Placa() {
  const { id } = useParams()
  const navegar = useNavigate()
  const qc = useQueryClient()
  const [viendo, setViendo] = useState<{ id: number; nombre: string; clave: string } | null>(null)
  const q = useQuery({ queryKey: ['placa', id], queryFn: () => api<PlacaFicha>(`/placas/${id}`) })
  const borrar = useMutation({
    mutationFn: () => api(`/placas/${id}`, { method: 'DELETE' }),
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: ['placas'] })
      void qc.invalidateQueries({ queryKey: ['placas-resumen'] })
      navegar('/placas')
    },
  })
  if (q.isPending) return <Loader />
  if (q.error) return <Alert color="red">{q.error.message}</Alert>
  const p = q.data

  return (
    <Stack maw={1100}>
      <Breadcrumbs>
        <Anchor component={Link} to="/placas">
          Placas
        </Anchor>
        {p.modelos
          .filter((m) => m.estado === 'confirmado')
          .slice(0, 1)
          .map((m) => (
            <Anchor key={m.id} component={Link} to={`/catalogo/${m.id}`}>
              {titulo(m)}
            </Anchor>
          ))}
      </Breadcrumbs>
      <Group align="flex-start" wrap="nowrap">
        <Miniatura archivoId={p.portada?.id ?? 0} clave={p.portada?.clave} tamano={512} lado={200} tipo="escena" ext="" />
        <Stack gap={6} style={{ flex: 1, minWidth: 0 }}>
          <Title order={2} style={{ wordBreak: 'break-word' }}>
            {p.favorita && (
              <Text span c="yellow" inherit>
                ★{' '}
              </Text>
            )}
            {p.nombre}
          </Title>
          <Group gap={4}>
            <Badge color={ESTADO_PLACA[p.estado].color} variant="light">
              {ESTADO_PLACA[p.estado].texto}
            </Badge>
            <Badge color="gray" variant="outline">
              {p.origen === 'manual' ? 'hecha a mano' : 'detectada'}
            </Badge>
          </Group>
          <Code block style={{ whiteSpace: 'pre-wrap', wordBreak: 'break-all' }}>
            {p.carpeta || '(raíz)'}
          </Code>
          {p.origen === 'manual' && (
            <Group>
              <Button size="xs" variant="subtle" color="red" loading={borrar.isPending} onClick={() => borrar.mutate()}>
                Borrar esta placa (sus archivos vuelven a detectarse solos)
              </Button>
            </Group>
          )}
          {borrar.error && <Alert color="red">{borrar.error.message}</Alert>}
        </Stack>
      </Group>
      <Modelos p={p} />
      <ConQue key={`${p.actualizada}-${p.impresora_id}-${p.perfil_id}-${p.material_id}`} p={p} />
      <Archivos p={p} ver={setViendo} />
      <Datos key={p.actualizada} p={p} />
      <Modal opened={viendo !== null} onClose={() => setViendo(null)} title={viendo?.nombre} size="lg" centered>
        {viendo && (
          <img
            src={`/api/v1/archivos/${viendo.id}/miniatura?t=512&v=${viendo.clave}`}
            alt={viendo.nombre}
            style={{ width: '100%', objectFit: 'contain' }}
          />
        )}
      </Modal>
    </Stack>
  )
}
