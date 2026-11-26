import {
  ActionIcon,
  Alert,
  Anchor,
  Badge,
  Button,
  ColorInput,
  Group,
  Loader,
  Modal,
  ScrollArea,
  Stack,
  Table,
  Text,
  TextInput,
  Title,
} from '@mantine/core'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { useState } from 'react'
import { Link } from 'react-router'

import { api } from '../api'
import { n } from '../formato'
import type { Etiqueta } from '../tipos'

function Editor({ e, todas, cerrar }: { e: Etiqueta; todas: Etiqueta[]; cerrar: () => void }) {
  const qc = useQueryClient()
  const [nombre, setNombre] = useState(e.nombre)
  const [tipo, setTipo] = useState(e.tipo)
  const [color, setColor] = useState(e.color)
  const une = todas.find((x) => x.id !== e.id && x.nombre === nombre.trim())
  const guardar = useMutation({
    mutationFn: () =>
      api(`/etiquetas/${e.id}`, { method: 'PATCH', body: JSON.stringify({ nombre, tipo, color }) }),
    onSuccess: () => {
      for (const k of ['etiquetas', 'modelos', 'filtros', 'modelo']) void qc.invalidateQueries({ queryKey: [k] })
      cerrar()
    },
  })
  return (
    <Stack>
      <TextInput label="Nombre" value={nombre} onChange={(x) => setNombre(x.currentTarget.value)} />
      {une && (
        <Alert color="yellow" variant="light">
          Ya existe «{une.nombre}» ({n(une.modelos)} modelos): al guardar se unen las dos y los {n(e.modelos)} modelos
          de esta pasan a aquella.
        </Alert>
      )}
      <TextInput
        label="Tipo"
        description="Libre: capítulo, equivalencia, proxy, línea…"
        value={tipo}
        onChange={(x) => setTipo(x.currentTarget.value)}
      />
      <ColorInput label="Color" value={color} onChange={setColor} format="hex" />
      {guardar.error && (
        <Alert color="red" variant="light">
          {guardar.error.message}
        </Alert>
      )}
      <Group justify="flex-end">
        <Button variant="default" onClick={cerrar}>
          Cancelar
        </Button>
        <Button onClick={() => guardar.mutate()} loading={guardar.isPending} color={une ? 'yellow' : undefined}>
          {une ? 'Unir' : 'Guardar'}
        </Button>
      </Group>
    </Stack>
  )
}

export default function Etiquetas() {
  const q = useQuery({ queryKey: ['etiquetas'], queryFn: () => api<Etiqueta[]>('/etiquetas') })
  const [filtro, setFiltro] = useState('')
  const [editando, setEditando] = useState<Etiqueta | null>(null)
  if (q.isPending) return <Loader />
  if (q.error) return <Alert color="red">{q.error.message}</Alert>
  const lista = q.data.filter((e) => e.nombre.toLowerCase().includes(filtro.toLowerCase()))

  return (
    <Stack maw={900}>
      <Group justify="space-between" align="flex-end">
        <Title order={2}>Etiquetas</Title>
        <Text c="dimmed" size="sm">
          {n(q.data.length)} etiquetas
        </Text>
      </Group>
      <TextInput placeholder="Buscar etiqueta…" value={filtro} onChange={(e) => setFiltro(e.currentTarget.value)} />
      <ScrollArea>
        <Table striped highlightOnHover miw={520}>
          <Table.Thead>
            <Table.Tr>
              <Table.Th>Etiqueta</Table.Th>
              <Table.Th>Tipo</Table.Th>
              <Table.Th ta="right">Modelos</Table.Th>
              <Table.Th />
            </Table.Tr>
          </Table.Thead>
          <Table.Tbody>
            {lista.map((e) => (
              <Table.Tr key={e.id}>
                <Table.Td>
                  <Badge variant="light" color={e.color || undefined}>
                    {e.nombre}
                  </Badge>
                </Table.Td>
                <Table.Td>
                  <Text size="sm" c="dimmed">
                    {e.tipo}
                  </Text>
                </Table.Td>
                <Table.Td ta="right">
                  <Anchor component={Link} to={`/catalogo?etiqueta=${encodeURIComponent(e.nombre)}`} size="sm">
                    {n(e.modelos)}
                  </Anchor>
                </Table.Td>
                <Table.Td ta="right">
                  <ActionIcon variant="subtle" aria-label={`Editar ${e.nombre}`} onClick={() => setEditando(e)}>
                    ✎
                  </ActionIcon>
                </Table.Td>
              </Table.Tr>
            ))}
          </Table.Tbody>
        </Table>
      </ScrollArea>
      <Modal opened={editando !== null} onClose={() => setEditando(null)} title="Editar etiqueta">
        {editando && <Editor e={editando} todas={q.data} cerrar={() => setEditando(null)} />}
      </Modal>
    </Stack>
  )
}
