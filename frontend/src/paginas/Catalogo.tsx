import {
  Alert,
  Badge,
  Button,
  Checkbox,
  Drawer,
  Group,
  Loader,
  Pagination,
  ScrollArea,
  Select,
  Stack,
  Table,
  Text,
  TextInput,
  Title,
  Tooltip,
} from '@mantine/core'
import { useDebouncedValue } from '@mantine/hooks'
import { keepPreviousData, useQuery } from '@tanstack/react-query'
import { useEffect, useState } from 'react'
import { useNavigate, useSearchParams } from 'react-router'

import { api } from '../api'
import EdicionLote from '../componentes/EdicionLote'
import Miniatura from '../componentes/Miniatura'
import { n, tamano, titulo } from '../formato'
import type { ListaModelos, OpcionesFiltros } from '../tipos'

const POR_PAGINA = 50
const CLAVES = [
  'q', 'juego', 'faccion', 'estudio', 'etiqueta', 'frio', 'revisado', 'pendiente', 'avisos', 'orden', 'pagina',
] as const

export default function Catalogo() {
  const [params, setParams] = useSearchParams()
  const navegar = useNavigate()
  const [texto, setTexto] = useState(params.get('q') ?? '')
  const [buscado] = useDebouncedValue(texto, 300)
  const [seleccion, setSeleccion] = useState<Set<number>>(new Set())
  const [lote, setLote] = useState(false)

  const poner = (cambios: Partial<Record<(typeof CLAVES)[number], string | null>>) => {
    const p = new URLSearchParams(params)
    for (const [k, v] of Object.entries(cambios)) {
      if (v) p.set(k, v)
      else p.delete(k)
    }
    if (!('pagina' in cambios)) p.delete('pagina')
    setParams(p, { replace: true })
  }

  useEffect(() => {
    if ((params.get('q') ?? '') !== buscado) poner({ q: buscado })
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [buscado])

  const consulta = new URLSearchParams()
  for (const k of CLAVES) {
    const v = params.get(k)
    if (v) consulta.set(k, v)
  }
  consulta.set('por_pagina', String(POR_PAGINA))

  const opciones = useQuery({
    queryKey: ['filtros'],
    queryFn: () => api<OpcionesFiltros>('/modelos/filtros'),
    staleTime: 60_000,
  })
  const lista = useQuery({
    queryKey: ['modelos', consulta.toString()],
    queryFn: () => api<ListaModelos>(`/modelos?${consulta}`),
    placeholderData: keepPreviousData,
  })

  const juego = params.get('juego')
  const facciones = (opciones.data?.facciones ?? [])
    .filter((f) => !juego || f.juego === juego)
    .map((f) => ({ value: f.nombre, label: `${f.nombre} (${f.modelos})` }))
  const unicas = Array.from(new Map(facciones.map((f) => [f.value, f])).values())
  const paginas = lista.data ? Math.max(1, Math.ceil(lista.data.total / POR_PAGINA)) : 1
  const enPagina = lista.data?.modelos.map((m) => m.id) ?? []
  const todosEnPagina = enPagina.length > 0 && enPagina.every((id) => seleccion.has(id))
  const alternar = (id: number) =>
    setSeleccion((s) => {
      const n = new Set(s)
      if (n.has(id)) n.delete(id)
      else n.add(id)
      return n
    })
  const estado = params.get('pendiente') ? 'pendiente' : params.get('avisos') ? 'avisos' : params.get('revisado') === 'no' ? 'sin_revisar' : null

  return (
    <Stack>
      <Group justify="space-between" align="flex-end">
        <Title order={2}>Catálogo</Title>
        <Group gap="xs">
          {lista.data && (
            <Text c="dimmed" size="sm">
              {n(lista.data.total)} modelos
            </Text>
          )}
          {seleccion.size > 0 && (
            <>
              <Button size="xs" onClick={() => setLote(true)}>
                Editar {seleccion.size} seleccionados
              </Button>
              <Button size="xs" variant="subtle" onClick={() => setSeleccion(new Set())}>
                Quitar selección
              </Button>
            </>
          )}
          <Button size="xs" variant="default" component="a" href="/api/v1/exportar/catalogo.csv">
            Exportar CSV
          </Button>
        </Group>
      </Group>

      <TextInput
        placeholder="Buscar por nombre, facción, estudio, etiqueta o notas…"
        value={texto}
        onChange={(e) => setTexto(e.currentTarget.value)}
      />
      <Group gap="xs" grow wrap="wrap">
        <Select
          placeholder="Juego"
          clearable
          data={(opciones.data?.juegos ?? []).map((j) => ({ value: j.nombre, label: `${j.nombre} (${j.modelos})` }))}
          value={juego}
          onChange={(v) => poner({ juego: v, faccion: null })}
          miw={160}
        />
        <Select
          placeholder="Facción / tipo"
          clearable
          searchable
          data={unicas}
          value={params.get('faccion')}
          onChange={(v) => poner({ faccion: v })}
          miw={180}
        />
        <Select
          placeholder="Estudio"
          clearable
          searchable
          data={(opciones.data?.estudios ?? []).map((e) => ({ value: e.nombre, label: `${e.nombre} (${e.modelos})` }))}
          value={params.get('estudio')}
          onChange={(v) => poner({ estudio: v })}
          miw={160}
        />
        <Select
          placeholder="Etiqueta"
          clearable
          searchable
          data={(opciones.data?.etiquetas ?? []).map((e) => ({ value: e.nombre, label: `${e.nombre} (${e.modelos})` }))}
          value={params.get('etiqueta')}
          onChange={(v) => poner({ etiqueta: v })}
          miw={160}
        />
        <Select
          placeholder="Frío"
          clearable
          data={[
            { value: 'con', label: 'Con copias en frío' },
            { value: 'sin', label: 'Sin copias en frío' },
          ]}
          value={params.get('frio')}
          onChange={(v) => poner({ frio: v })}
          miw={150}
        />
        <Select
          placeholder="Estado"
          clearable
          data={[
            { value: 'pendiente', label: 'Pendientes de mover' },
            { value: 'avisos', label: 'Con avisos' },
            { value: 'sin_revisar', label: 'Sin revisar' },
          ]}
          value={estado}
          onChange={(v) =>
            poner({
              pendiente: v === 'pendiente' ? 'si' : null,
              avisos: v === 'avisos' ? 'si' : null,
              revisado: v === 'sin_revisar' ? 'no' : null,
            })
          }
          miw={160}
        />
        <Select
          placeholder="Orden"
          data={[
            { value: 'nombre', label: 'Por juego y nombre' },
            { value: 'tamano', label: 'Más pesados' },
            { value: 'archivos', label: 'Más archivos' },
          ]}
          value={params.get('orden') ?? 'nombre'}
          onChange={(v) => poner({ orden: v === 'nombre' ? null : v })}
          miw={150}
        />
      </Group>

      {lista.error && <Alert color="red">{lista.error.message}</Alert>}
      {lista.isPending ? (
        <Loader />
      ) : (
        <ScrollArea>
          <Table highlightOnHover striped miw={760} style={{ opacity: lista.isPlaceholderData ? 0.6 : 1 }}>
            <Table.Thead>
              <Table.Tr>
                <Table.Th w={36}>
                  <Checkbox
                    aria-label="Seleccionar la página"
                    checked={todosEnPagina}
                    indeterminate={!todosEnPagina && enPagina.some((id) => seleccion.has(id))}
                    onChange={() =>
                      setSeleccion((s) => {
                        const n = new Set(s)
                        for (const id of enPagina) {
                          if (todosEnPagina) n.delete(id)
                          else n.add(id)
                        }
                        return n
                      })
                    }
                  />
                </Table.Th>
                <Table.Th w={60} />
                <Table.Th>Modelo</Table.Th>
                <Table.Th>Facción / tipo</Table.Th>
                <Table.Th>Estudio</Table.Th>
                <Table.Th>Etiquetas</Table.Th>
                <Table.Th ta="right">Archivos</Table.Th>
                <Table.Th ta="right">Tamaño</Table.Th>
              </Table.Tr>
            </Table.Thead>
            <Table.Tbody>
              {lista.data?.modelos.map((m) => (
                <Table.Tr key={m.id} onClick={() => navegar(`/catalogo/${m.id}`)} style={{ cursor: 'pointer' }}>
                  <Table.Td onClick={(e) => e.stopPropagation()}>
                    <Checkbox aria-label="Seleccionar" checked={seleccion.has(m.id)} onChange={() => alternar(m.id)} />
                  </Table.Td>
                  <Table.Td>
                    {m.portada ? (
                      <Miniatura archivoId={m.portada.id} clave={m.portada.clave} lado={48} />
                    ) : (
                      <Miniatura archivoId={0} clave={null} lado={48} tipo="otro" ext="" />
                    )}
                  </Table.Td>
                  <Table.Td>
                    <Text size="sm" fw={500}>
                      {titulo(m)}
                    </Text>
                    <Group gap={4}>
                      {m.pendiente && (
                        <Badge size="xs" color="orange" variant="light">
                          por mover
                        </Badge>
                      )}
                      {m.n_avisos > 0 && (
                        <Badge size="xs" color="yellow" variant="light">
                          {m.n_avisos} {m.n_avisos === 1 ? 'aviso' : 'avisos'}
                        </Badge>
                      )}
                      {!m.revisado && (
                        <Badge size="xs" color="gray" variant="outline">
                          sin revisar
                        </Badge>
                      )}
                    </Group>
                    {m.subcarpeta && (
                      <Text size="xs" c="dimmed">
                        {m.subcarpeta}
                      </Text>
                    )}
                  </Table.Td>
                  <Table.Td>
                    <Text size="sm">{m.faccion}</Text>
                    <Text size="xs" c="dimmed">
                      {m.juego}
                    </Text>
                  </Table.Td>
                  <Table.Td>
                    <Text size="sm">{m.estudio}</Text>
                  </Table.Td>
                  <Table.Td>
                    <Group gap={4}>
                      {m.etiquetas.slice(0, 3).map((e) => (
                        <Badge key={e} size="xs" variant="light">
                          {e}
                        </Badge>
                      ))}
                      {m.etiquetas.length > 3 && (
                        <Tooltip label={m.etiquetas.slice(3).join(' · ')}>
                          <Badge size="xs" variant="outline" color="gray">
                            +{m.etiquetas.length - 3}
                          </Badge>
                        </Tooltip>
                      )}
                    </Group>
                  </Table.Td>
                  <Table.Td ta="right">
                    <Text size="sm">{n(m.n_archivos)}</Text>
                  </Table.Td>
                  <Table.Td ta="right">
                    <Text size="sm">{tamano(m.bytes)}</Text>
                    {m.n_frio > 0 && (
                      <Badge size="xs" variant="light" color="blue">
                        +{tamano(m.bytes_frio)} frío
                      </Badge>
                    )}
                  </Table.Td>
                </Table.Tr>
              ))}
            </Table.Tbody>
          </Table>
          {lista.data?.modelos.length === 0 && (
            <Text c="dimmed" ta="center" py="xl">
              Nada coincide con la búsqueda.
            </Text>
          )}
        </ScrollArea>
      )}
      {paginas > 1 && (
        <Group justify="center">
          <Pagination
            total={paginas}
            value={Number(params.get('pagina') ?? 1)}
            onChange={(p) => poner({ pagina: p > 1 ? String(p) : null })}
          />
        </Group>
      )}
      <Drawer opened={lote} onClose={() => setLote(false)} title="Editar varios modelos" position="right" size="md">
        {lote && (
          <EdicionLote
            ids={[...seleccion]}
            alTerminar={() => {
              setLote(false)
              setSeleccion(new Set())
            }}
          />
        )}
      </Drawer>
    </Stack>
  )
}
